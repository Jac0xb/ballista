use super::task::task_action::TaskAction;

pub struct ExecutionFrame<'a> {
    pub current_index: u8,
    pub depth: u8,
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
    pub fn get_current_action(&self) -> &TaskAction {
        self.actions.get(self.current_index as usize)
    }

    #[inline(always)]
    pub fn get_action(&self, index: u8) -> &TaskAction {
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
    fn get(&self, index: usize) -> &TaskAction;
}

impl ActionCollection for [TaskAction] {
    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn get(&self, index: usize) -> &TaskAction {
        &self[index]
    }
}

impl ActionCollection for &[TaskAction] {
    fn len(&self) -> usize {
        (*self).len()
    }

    fn is_empty(&self) -> bool {
        (*self).is_empty()
    }

    fn get(&self, index: usize) -> &TaskAction {
        &self[index]
    }
}

impl ActionCollection for &[&TaskAction] {
    fn len(&self) -> usize {
        (*self).len()
    }

    fn is_empty(&self) -> bool {
        (*self).is_empty()
    }

    fn get(&self, index: usize) -> &TaskAction {
        self[index]
    }
}

impl ActionCollection for &[Box<TaskAction>] {
    fn len(&self) -> usize {
        (*self).len()
    }

    fn is_empty(&self) -> bool {
        (*self).is_empty()
    }

    fn get(&self, index: usize) -> &TaskAction {
        &(*self)[index]
    }
}

impl ActionCollection for Box<TaskAction> {
    fn len(&self) -> usize {
        1
    }

    fn is_empty(&self) -> bool {
        false
    }

    fn get(&self, _index: usize) -> &TaskAction {
        self
    }
}
