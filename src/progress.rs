pub(crate) struct TaskProgress<'a> {
    pub(crate) name: &'a str,
    pub(crate) failed: bool,
}

impl<'a> TaskProgress<'a> {
    pub(crate) fn new(name: &'a str) -> Self {
        eprintln!("Fetching {} ...", name);
        TaskProgress {
            name,
            failed: false,
        }
    }
    pub(crate) fn fail(&mut self) { self.failed = true; }
}

impl Drop for TaskProgress<'_> {
    fn drop(&mut self) {
        let status = match self.failed {
            true => "failed",
            false => "done",
        };
        eprintln!("... '{}' {}.", self.name, status);
    }
}

