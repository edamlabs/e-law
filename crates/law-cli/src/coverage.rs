use law_model::{Prose, Rule};

use crate::cli::CoverageArgs;
use crate::exit::Exit;

impl CoverageArgs {
    fn measured(&self) -> Result<Exit, Exit> {
        let opened = self.project.open(&self.catalogues)?;
        let catalogues = &opened.loaded.catalogues;
        let rules: Vec<&Rule> = catalogues
            .iter()
            .flat_map(|catalogue| &catalogue.rules)
            .filter(|rule| opened.selection.admits(rule))
            .collect();
        let prose: Vec<&Prose> = catalogues.iter().flat_map(|catalogue| &catalogue.prose).collect();
        Exit::emit(|out| law_report::coverage(&self.by, &rules, &prose, self.output_format, out))?;
        Ok(Exit::Clean)
    }

    /// The selected rules counted by the values of a tag key, then what stays prose; no project needed with
    /// `--catalogue`.
    #[must_use]
    pub fn run(&self) -> Exit {
        self.measured().unwrap_or_else(|exit| exit)
    }
}
