use anyhow::Result;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use space::{
    app::{self, ScanProgress},
    classify::Rule,
    cli::Cli,
    model::{Estimate, StorageReport},
    output::terminal::{self, RenderOptions},
    scan::{merge, owning_volume_index},
};
use std::{collections::BTreeMap, sync::Mutex, time::Duration};

const TICK: Duration = Duration::from_millis(100);
const CHILD_WIDTH: usize = 12;

struct Section {
    bar: ProgressBar,
    pending: usize,
    sizes: BTreeMap<&'static str, Estimate>,
}

impl Section {
    fn refresh(&self, name: &str, plain: &ProgressStyle, options: RenderOptions) {
        if self.pending == 0 {
            self.bar.disable_steady_tick();
            if self.sizes.is_empty() {
                self.bar.finish_and_clear();
                return;
            }
            self.bar.set_style(plain.clone());
        }

        let total = (!self.sizes.is_empty()).then(|| {
            let mut total = self
                .sizes
                .values()
                .fold(Estimate::exact(0), |total, size| merge(&total, size));
            total.incomplete |= self.pending > 0 && total.bytes > 0;
            total
        });
        let mut children: Vec<_> = self
            .sizes
            .iter()
            .map(|(&name, size)| (name, size))
            .collect();
        children.sort_by_key(|(_, size)| std::cmp::Reverse(size.bytes));
        self.bar.set_message(terminal::section_lines(
            name,
            total.as_ref(),
            &children,
            CHILD_WIDTH,
            options,
        ));
        if self.pending == 0 {
            self.bar.force_draw();
        }
    }
}

struct Volume {
    header: ProgressBar,
    sections: BTreeMap<&'static str, Section>,
}

struct Dashboard {
    progress: MultiProgress,
    report: StorageReport,
    volumes: Vec<Volume>,
    options: RenderOptions,
    plain: ProgressStyle,
    spinning: ProgressStyle,
    project_home: Option<usize>,
    project_search_timed_out: bool,
}

impl Dashboard {
    fn new(
        progress: MultiProgress,
        report: &StorageReport,
        options: RenderOptions,
        plain: ProgressStyle,
        spinning: ProgressStyle,
    ) -> Self {
        let volumes: Vec<_> = terminal::volume_headers(report, options)
            .into_iter()
            .map(|header| {
                let bar = progress.add(ProgressBar::new_spinner().with_style(plain.clone()));
                bar.set_message(header.trim_end_matches('\n').to_owned());
                Volume {
                    header: bar,
                    sections: BTreeMap::new(),
                }
            })
            .collect();
        if let Some(last) = volumes.last() {
            last.header.force_draw();
        }
        Self {
            progress,
            report: report.clone(),
            volumes,
            options,
            plain,
            spinning,
            project_home: None,
            project_search_timed_out: false,
        }
    }

    fn register(&mut self, index: usize, name: &'static str) {
        let volume = &mut self.volumes[index];
        let before = volume
            .sections
            .range(..name)
            .next_back()
            .map_or(&volume.header, |(_, section)| &section.bar)
            .clone();
        let section = volume.sections.entry(name).or_insert_with(|| {
            let bar = self.progress.insert_after(
                &before,
                ProgressBar::new_spinner().with_style(self.spinning.clone()),
            );
            bar.set_message(terminal::section_lines(
                name,
                None,
                &[],
                CHILD_WIDTH,
                self.options,
            ));
            bar.enable_steady_tick(TICK);
            Section {
                bar,
                pending: 0,
                sizes: BTreeMap::new(),
            }
        });
        section.pending += 1;
    }

    fn register_rules(&mut self, rules: &[Rule]) {
        for rule in rules {
            if let Some(index) = owning_volume_index(&self.report, &rule.path) {
                self.register(index, rule.category);
            }
        }
    }

    fn update(&mut self, event: ScanProgress<'_>) {
        match event {
            ScanProgress::Capacity(_) => {}
            ScanProgress::Started { known, home } => {
                self.register_rules(known);
                self.project_home = owning_volume_index(&self.report, home);
                if let Some(index) = self.project_home {
                    self.register(index, "Projects");
                }
            }
            ScanProgress::ProjectsFound { rules, timed_out } => {
                self.project_search_timed_out = timed_out;
                self.register_rules(rules);
                if let Some(index) = self.project_home.take() {
                    self.finish(index, "Projects", "Projects", None);
                }
            }
            ScanProgress::FolderFinished { rule, estimate } => {
                if let Some(index) = owning_volume_index(&self.report, &rule.path) {
                    self.finish(index, rule.category, rule.name, estimate);
                }
            }
        }
    }

    fn finish(
        &mut self,
        index: usize,
        category: &str,
        name: &'static str,
        estimate: Option<&Estimate>,
    ) {
        let section = self.volumes[index]
            .sections
            .get_mut(category)
            .expect("registered section");
        section.pending -= 1;
        if let Some(estimate) = estimate.filter(|size| size.bytes > 0 || size.incomplete) {
            let mut estimate = estimate.clone();
            if category == "Projects" && self.project_search_timed_out {
                estimate.incomplete = true;
                estimate.timed_out = true;
            }
            section
                .sizes
                .entry(name)
                .and_modify(|size| *size = merge(size, &estimate))
                .or_insert(estimate);
            section.refresh(category, &self.plain, self.options);
        } else if section.pending == 0 {
            section.refresh(category, &self.plain, self.options);
        }
    }

    fn clear(self) {
        let bars = || {
            self.volumes.iter().flat_map(|volume| {
                std::iter::once(&volume.header)
                    .chain(volume.sections.values().map(|section| &section.bar))
            })
        };
        for bar in bars() {
            bar.disable_steady_tick();
        }
        let _ = self.progress.clear();
        for bar in bars() {
            self.progress.remove(bar);
        }
    }
}

pub fn run(cli: &Cli, options: RenderOptions) -> Result<app::Outcome> {
    let progress = MultiProgress::with_draw_target(ProgressDrawTarget::stdout());
    let plain = ProgressStyle::with_template("{msg}")?;
    let spinning = ProgressStyle::with_template("{msg}\n    {spinner:.cyan} Scanning…")?;
    let discovering = progress.add(ProgressBar::new_spinner().with_style(
        ProgressStyle::with_template("{spinner:.cyan} Reading disk capacity…")?,
    ));
    discovering.enable_steady_tick(TICK);
    let dashboard = Mutex::new(None::<Dashboard>);
    let outcome = app::run_with_progress(cli, |event| {
        let mut dashboard = dashboard.lock().expect("dashboard state poisoned");
        if let ScanProgress::Capacity(report) = event {
            discovering.finish_and_clear();
            progress.remove(&discovering);
            *dashboard = Some(Dashboard::new(
                progress.clone(),
                report,
                options,
                plain.clone(),
                spinning.clone(),
            ));
        } else if let Some(dashboard) = dashboard.as_mut() {
            dashboard.update(event);
        }
    });
    discovering.finish_and_clear();
    progress.remove(&discovering);
    if let Some(dashboard) = dashboard.into_inner().expect("dashboard state poisoned") {
        dashboard.clear();
    }
    outcome
}
