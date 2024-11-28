#[macro_export]
macro_rules! debug_msg {
    ($($arg:tt)*) => {
        if false {
            pinocchio::msg!($($arg)*)
        }
    };
}
