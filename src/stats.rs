#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Stats {
    failure: usize,
    success: usize,
}

impl Stats {
    pub fn new() -> Stats {
        Stats {
            failure: 0,
            success: 0,
        }
    }

    pub fn inc_success(&mut self) {
        self.success += 1;
    }

    pub fn inc_failure(&mut self) {
        self.failure += 1;
    }

    pub fn success(&self) -> usize {
        self.success
    }

    pub fn failure(&self) -> usize {
        self.failure
    }
}
