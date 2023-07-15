use std::time::Instant;

pub(crate) struct TaskProgress<'a> {
    pub(crate) name: &'a str,
    pub(crate) failed: bool,
    started: Instant,
}

impl<'a> TaskProgress<'a> {
    pub(crate) fn new(name: &'a str) -> Self {
        eprintln!("Fetching '{}'", name);
        TaskProgress {
            name,
            failed: false,
            started: std::time::Instant::now(),
        }
    }
    pub(crate) fn fail(&mut self) {
        self.failed = true;
    }
}

impl Drop for TaskProgress<'_> {
    fn drop(&mut self) {
        let status = match self.failed {
            true => "failed",
            false => "done",
        };
        let took = self.started.elapsed().as_secs_f64();
        eprintln!("'{}' {}, took {:.2} s.", self.name, status, took);
    }
}
