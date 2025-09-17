/*
Util for dealing with Infalliable results without using unwrap
*/

use core::convert::Infallible;

pub trait InfallibleExt<T> {
    fn unwrap_infallible(self) -> T;
}

impl<T> InfallibleExt<T> for Result<T, Infallible> {
    fn unwrap_infallible(self) -> T {
        self.unwrap_or_else(|_| unreachable!())
    }
}
