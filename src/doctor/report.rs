use super::check::{Outcome, Status};

pub struct Report<'a> {
    pub entries: &'a [(&'static str, Outcome)],
}

impl Report<'_> {
    pub fn print(&self) {
        for (name, outcome) in self.entries {
            println!(
                "{} {:<12} {}",
                symbol(outcome.status),
                name,
                outcome.summary
            );
        }

        let verdicts: Vec<&str> = self
            .entries
            .iter()
            .filter_map(|(_, o)| o.verdict.as_deref())
            .collect();

        if !verdicts.is_empty() {
            println!();
            for v in verdicts {
                println!("-> {}", v);
            }
        }
    }
}

fn symbol(status: Status) -> &'static str {
    match status {
        Status::Ok => "[ok]  ",
        Status::Warn => "[warn]",
        Status::Fail => "[fail]",
        Status::Skip => "[skip]",
    }
}
