use num_enum::{IntoPrimitive, TryFromPrimitive};

pub mod task_definition;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum BallistaAccount {
    TaskDefinition = 0,
}
