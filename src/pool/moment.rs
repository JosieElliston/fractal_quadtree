use std::ops;

#[repr(transparent)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, bytemuck::NoUninit)]
/// uses signed integer internally but should be nonnegative.
/// i32 mean it'll overflow after 19884 hours at 60 fps.
pub(crate) struct MomentInner(i32);

impl MomentInner {
    pub(crate) const MIN: Self = Self(0);

    fn new(value: i32) -> Self {
        debug_assert!(value >= 0);
        Self(value)
    }

    pub(crate) fn uninit() -> Self {
        Self(-987123)
        // Self::default()
    }
}
impl ops::Add<i32> for MomentInner {
    type Output = Self;

    fn add(self, rhs: i32) -> Self {
        Self::new(self.0 + rhs)
    }
}
impl ops::AddAssign<i32> for MomentInner {
    fn add_assign(&mut self, rhs: i32) {
        *self = Self::new(self.0 + rhs);
    }
}
impl ops::Sub<i32> for MomentInner {
    type Output = Self;

    fn sub(self, rhs: i32) -> Self {
        Self::new(self.0 - rhs)
    }
}
