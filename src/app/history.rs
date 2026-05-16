#[derive(Debug, Default)]
pub(super) struct CommandHistory {
  entries: Vec<String>,
  index: Option<usize>,
  draft: String,
}

impl CommandHistory {
  pub(super) fn new() -> Self {
    Self::default()
  }

  pub(super) fn record(&mut self, input: &str) {
    if input.is_empty() {
      return;
    }
    if self.entries.last().is_some_and(|entry| entry == input) {
      return;
    }
    self.entries.push(input.to_string());
  }

  pub(super) fn previous(&mut self, current_input: &str) -> Option<String> {
    if self.entries.is_empty() {
      return None;
    }

    match self.index {
      None => {
        self.draft = current_input.to_string();
        self.index = Some(self.entries.len() - 1);
      }
      Some(index) => {
        if index > 0 {
          self.index = Some(index - 1);
        }
      }
    }

    self.index.map(|index| self.entries[index].clone())
  }

  pub(super) fn next(&mut self) -> Option<String> {
    let index = self.index?;

    if index + 1 < self.entries.len() {
      let next = index + 1;
      self.index = Some(next);
      return Some(self.entries[next].clone());
    }

    self.index = None;
    let draft = self.draft.clone();
    self.draft.clear();
    Some(draft)
  }

  pub(super) fn leave_navigation(&mut self) {
    self.index = None;
    self.draft.clear();
  }
}
