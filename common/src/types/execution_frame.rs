use super::task::command::Command;

pub struct ExecutionFrame<'a> {
    pub current_index: u8,
    pub actions: &'a dyn ActionCollection,
}

impl<'a> ExecutionFrame<'a> {
    #[inline(always)]
    pub fn increment(&mut self, increment: u8) {
        self.current_index += increment;
    }

    #[inline(always)]
    pub fn set_index(&mut self, index: u8) {
        self.current_index = index;
    }

    #[inline(always)]
    pub fn get_index(&self) -> u8 {
        self.current_index
    }

    #[inline(always)]
    pub fn get_current_action(&self) -> &Command {
        self.actions.get(self.current_index as usize)
    }

    #[inline(always)]
    pub fn get_action(&self, index: u8) -> &Command {
        self.actions.get(index as usize)
    }

    #[inline(always)]
    pub fn actions_len(&self) -> usize {
        self.actions.len()
    }
}

pub trait ActionCollection {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn get(&self, index: usize) -> &Command;
}

impl ActionCollection for [Command] {
    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn get(&self, index: usize) -> &Command {
        &self[index]
    }
}

impl ActionCollection for &[Command] {
    fn len(&self) -> usize {
        (*self).len()
    }

    fn is_empty(&self) -> bool {
        (*self).is_empty()
    }

    fn get(&self, index: usize) -> &Command {
        &self[index]
    }
}

impl ActionCollection for &[&Command] {
    fn len(&self) -> usize {
        (*self).len()
    }

    fn is_empty(&self) -> bool {
        (*self).is_empty()
    }

    fn get(&self, index: usize) -> &Command {
        self[index]
    }
}

impl ActionCollection for &[Box<Command>] {
    fn len(&self) -> usize {
        (*self).len()
    }

    fn is_empty(&self) -> bool {
        (*self).is_empty()
    }

    fn get(&self, index: usize) -> &Command {
        &(*self)[index]
    }
}

impl ActionCollection for Box<Command> {
    fn len(&self) -> usize {
        1
    }

    fn is_empty(&self) -> bool {
        false
    }

    fn get(&self, _index: usize) -> &Command {
        self
    }
}
