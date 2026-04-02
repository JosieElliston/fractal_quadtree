use std::{
    fmt::{self, Display},
    ops::{Add, AddAssign, Neg, Sub, SubAssign},
};

/// a fixed point number in [-RANGE, RANGE)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Fixed(i64);
pub(crate) type Real = Fixed;
pub(crate) type Imag = Fixed;

impl Fixed {
    const SHIFT: u32 = 58;
    const RANGE: f64 = 32.0;

    const MIN: Self = Self(i64::MIN);
    const MAX: Self = Self(i64::MAX);
    pub(crate) const ZERO: Self = Self(0);
    // pub(crate) const ONE: Self = Self(1 << Self::SHIFT);
    // pub(crate) const TWO: Self = Self(2 << Self::SHIFT);
    // pub(crate) const NEG_FOUR: Self = Self::MIN;

    pub(crate) fn try_from_f64(f: f64) -> Option<Self> {
        if !(-Fixed::RANGE..Fixed::RANGE).contains(&f) {
            None
        } else {
            Some(Self((f * (1_i64 << Self::SHIFT) as f64) as i64))
        }
    }
    pub(crate) fn try_from_f32(f: f32) -> Option<Self> {
        Self::try_from_f64(f as f64)
    }
    pub(crate) fn into_f64(self) -> f64 {
        f64::from(self)
    }
    pub(crate) fn into_f32(self) -> f32 {
        f32::from(self)
    }

    /// note that this is approximate
    pub(crate) fn lerp(lo: Self, hi: Self, t: f64) -> Self {
        assert!(lo < hi);
        // assert!((0.0..=1.0).contains(&t));
        // lo * (1.0 - t) + hi * t
        lo + (hi - lo).mul_f64(t)
    }
    /// note that this is approximate
    pub(crate) fn inv_lerp(lo: Self, hi: Self, x: Self) -> f64 {
        assert!(lo < hi);
        // assert!((lo..=hi).contains(&x));
        f64::from(x - lo) / f64::from(hi - lo)
    }

    pub(crate) fn min(self, other: Self) -> Self {
        if self < other { self } else { other }
    }
    pub(crate) fn max(self, other: Self) -> Self {
        if self > other { self } else { other }
    }
    pub(crate) fn abs(self) -> Self {
        if self.0 < 0 { Self(-self.0) } else { self }
    }

    // /// divide self by 2
    // pub(crate) fn div2(self) -> Self {
    //     Self(self.0 / 2)
    // }
    /// divide self by 2, None if self is odd
    pub(crate) fn div2_exact(self) -> Option<Self> {
        if self.0 % 2 != 0 {
            None
        } else {
            Some(Self(self.0 / 2))
        }
    }
    pub(crate) fn div2_floor(self) -> Self {
        Self(self.0 / 2)
    }
    pub(crate) fn div2_floor_n(mut self, n: u32) -> Self {
        for _ in 0..n {
            self = self.div2_floor();
        }
        self
    }
    pub(crate) fn mul2(self) -> Self {
        Self(self.0 * 2)
    }
    pub(crate) fn mul2_n(mut self, n: u32) -> Self {
        for _ in 0..n {
            self = self.mul2();
        }
        self
    }

    #[track_caller]
    pub(crate) fn mul(self, other: Self) -> Self {
        Self(
            (self.0 as i128)
                .checked_mul(other.0 as i128)
                .unwrap()
                .checked_shr(Self::SHIFT)
                .unwrap()
                .try_into()
                .unwrap(),
        )
    }
    pub(crate) fn mul_f64(self, f: f64) -> Self {
        self.mul(f.into())
    }
    pub(crate) fn div_f64(self, f: f64) -> Self {
        self.mul(f.recip().into())
    }
    pub(crate) fn mul_f32(self, f: f32) -> Self {
        self.mul(f.into())
    }
    pub(crate) fn div_f32(self, f: f32) -> Self {
        self.mul(f.recip().into())
    }

    /// returns None if the length is zero
    /// TODO: returns None if it can't be represented as Fixed
    pub(crate) fn normalized(real: Real, imag: Imag) -> Option<(Real, Imag) >{
        let length = (real.into_f64() * real.into_f64() + imag.into_f64() * imag.into_f64()).sqrt();
        if length == 0.0 {
            None
        } else {
            Some((real.div_f64(length), imag.div_f64(length)))
        }
    }
}

impl Display for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format!("{:.6}", f64::from(*self)))
    }
}
impl fmt::Debug for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format!("0x{:016x}", self.0))
    }
}

impl From<Fixed> for f64 {
    fn from(f: Fixed) -> Self {
        (f.0 as f64) / (1_i64 << Fixed::SHIFT) as f64
    }
}
impl From<f64> for Fixed {
    #[track_caller]
    fn from(f: f64) -> Self {
        Self::try_from_f64(f).unwrap()
    }
}
impl From<Fixed> for f32 {
    fn from(f: Fixed) -> Self {
        f64::from(f) as f32
    }
}
impl From<f32> for Fixed {
    #[track_caller]
    fn from(f: f32) -> Self {
        Self::try_from_f32(f).unwrap()
    }
}

impl Add for Fixed {
    type Output = Self;

    #[track_caller]
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}
impl AddAssign for Fixed {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}
impl Sub for Fixed {
    type Output = Self;

    #[track_caller]
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}
impl SubAssign for Fixed {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}
impl Neg for Fixed {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero() {
        assert_eq!(Fixed::ZERO, Fixed::from(0.0));
    }

    #[test]
    fn test_one() {
        assert_eq!(Fixed(1_i64 << Fixed::SHIFT), Fixed::from(1.0));
    }

    #[test]
    fn test_range_neg() {
        assert_eq!(Fixed::from(-Fixed::RANGE), Fixed::MIN);
        assert_ne!(Fixed::from(-Fixed::RANGE / 32.0), Fixed::MIN);
        assert_ne!(Fixed::from(-Fixed::RANGE / 16.0), Fixed::MIN);
        assert_ne!(Fixed::from(-Fixed::RANGE / 8.0), Fixed::MIN);
        assert_ne!(Fixed::from(-Fixed::RANGE / 4.0), Fixed::MIN);
        assert_ne!(Fixed::from(-Fixed::RANGE / 2.0), Fixed::MIN);
        assert_ne!(Fixed::from(-Fixed::RANGE / 1.5), Fixed::MIN);
        assert_ne!(Fixed::from(-Fixed::RANGE / 3.0), Fixed::MIN);
        assert_eq!(Fixed::try_from_f64(-Fixed::RANGE * 2.0), None);
    }

    #[test]
    #[should_panic]
    fn test_range_very_neg() {
        assert_ne!(Fixed::from(-Fixed::RANGE * 2.0), Fixed::MIN);
    }

    #[test]
    fn test_range_pos() {
        assert_ne!(Fixed::try_from_f64(Fixed::RANGE / 32.0), None);
        assert_ne!(Fixed::try_from_f64(Fixed::RANGE / 16.0), None);
        assert_ne!(Fixed::try_from_f64(Fixed::RANGE / 8.0), None);
        assert_ne!(Fixed::try_from_f64(Fixed::RANGE / 4.0), None);
        assert_ne!(Fixed::try_from_f64(Fixed::RANGE / 2.0), None);
        assert_ne!(Fixed::try_from_f64(Fixed::RANGE / 1.5), None);
        assert_ne!(Fixed::try_from_f64(Fixed::RANGE / 3.0), None);
    }

    #[test]
    #[should_panic]
    fn test_range_very_pos() {
        assert_eq!(Fixed::from(Fixed::RANGE), Fixed::MIN);
    }

    #[test]
    fn test_identity_f64_fixed() {
        for f in [
            20.601, 20.617, -3.980, -22.092, -21.047, -19.458, 30.177, 20.944, 14.705, -16.787,
            20.603, -24.559, -18.584, -16.767, -6.018, -17.405, -15.284, 16.983, 22.734, -21.853,
            -27.686, -1.317, 17.825, -2.914, -18.354, -14.570, 12.935, 3.635, -9.842, -8.893,
        ] {
            let actual = f64::from(Fixed::from(f));
            assert_eq!(f, actual);
        }
    }

    #[test]
    fn test_identity_fixed_f64() {
        for f in [
            20.601, 20.617, -3.980, -22.092, -21.047, -19.458, 30.177, 20.944, 14.705, -16.787,
            20.603, -24.559, -18.584, -16.767, -6.018, -17.405, -15.284, 16.983, 22.734, -21.853,
            -27.686, -1.317, 17.825, -2.914, -18.354, -14.570, 12.935, 3.635, -9.842, -8.893,
        ]
        .map(Fixed::from)
        {
            let actual = Fixed::from(f64::from(f));
            assert_eq!(f, actual);
        }
    }
}
