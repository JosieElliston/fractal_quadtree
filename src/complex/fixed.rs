// pub(crate) mod exact {
//     use std::{fmt, ops};

//     use super::inexact::Fixed;

//     #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
//     pub(crate) struct ExactFixed(Fixed);
//     pub(crate) type ExactReal = ExactFixed;
//     pub(crate) type ExactImag = ExactFixed;
//     // pub(crate) type ExactComplex = (ExactReal, ExactImag);

//     impl ExactFixed {
//         pub(crate) const ZERO: Self = Self(Fixed::ZERO);
//         pub(crate) const ONE: Self = Self(Fixed::ONE);

//         /// computes self * 2, returns `None` if it would overflow
//         pub(crate) fn mul2_checked(self) -> Option<Self> {
//             Some(Self(self.0.mul2_checked()?))
//         }
//         /// computes self * 2, panics on overflow
//         pub(crate) fn mul2(self) -> Self {
//             Self(self.0.mul2())
//         }
//         /// computes self * 2^n, returns `None` if we would overflow
//         pub(crate) fn mul2_n_checked(self, n: u32) -> Option<Self> {
//             Some(Self(self.0.mul2_n_checked(n)?))
//         }
//         /// computes self * 2^n, panics on overflow
//         pub(crate) fn mul2_n(self, n: u32) -> Self {
//             Self(self.0.mul2_n(n))
//         }

//         /// computes self / 2, returns `None` if we would lose precision (ie if self is odd)
//         pub(crate) fn div2_exact_checked(self) -> Option<Self> {
//             Some(Self(self.0.div2_exact_checked()?))
//         }
//         /// computes self / 2, panics if we would lose precision (ie if self is odd)
//         pub(crate) fn div2_exact(self) -> Self {
//             Self(self.0.div2_exact())
//         }
//         /// computes self / 2^n, returns `None` if we would lose precision
//         pub(crate) fn div2_n_exact_checked(self, n: u32) -> Option<Self> {
//             Some(Self(self.0.div2_n_exact_checked(n)?))
//         }
//         /// computes self / 2^n, panics if we would lose precision
//         pub(crate) fn div2_n_exact(self, n: u32) -> Self {
//             Self(self.0.div2_n_exact(n))
//         }
//     }
//     impl From<ExactFixed> for Fixed {
//         fn from(f: ExactFixed) -> Self {
//             f.0
//         }
//     }
//     impl ops::Add<ExactFixed> for ExactFixed {
//         type Output = Self;

//         fn add(self, rhs: ExactFixed) -> Self::Output {
//             Self(self.0 + rhs.0)
//         }
//     }
//     impl ops::AddAssign<ExactFixed> for ExactFixed {
//         fn add_assign(&mut self, rhs: ExactFixed) {
//             self.0 += rhs.0;
//         }
//     }
//     impl ops::Sub<ExactFixed> for ExactFixed {
//         type Output = Self;

//         fn sub(self, rhs: ExactFixed) -> Self::Output {
//             Self(self.0 - rhs.0)
//         }
//     }
//     impl ops::SubAssign<ExactFixed> for ExactFixed {
//         fn sub_assign(&mut self, rhs: ExactFixed) {
//             self.0 -= rhs.0;
//         }
//     }
// }

// pub(crate) mod inexact {
use std::{fmt, ops};

/// a fixed point number in [-DOMAIN_RADIUS, DOMAIN_RADIUS)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Fixed(i64);
pub(crate) type Real = Fixed;
pub(crate) type Imag = Fixed;
// pub(crate) type Complex = (Real, Imag);

impl Fixed {
    const SHIFT: u32 = 58;
    const DOMAIN_RADIUS: f64 = 32.0;

    pub(crate) const MIN: Self = Self(i64::MIN);
    pub(crate) const MAX: Self = Self(i64::MAX);
    pub(crate) const ZERO: Self = Self(0);
    pub(crate) const ONE: Self = Self(1 << Self::SHIFT);
    // pub(crate) const TWO: Self = Self(2 << Self::SHIFT);
    // pub(crate) const NEG_FOUR: Self = Self::MIN;

    pub(crate) const fn in_domain(f: f64) -> bool {
        -Self::DOMAIN_RADIUS <= f && f < Self::DOMAIN_RADIUS
    }

    /// returns `None` if `f` is outside the domain
    pub(crate) const fn try_from_f64(f: f64) -> Option<Self> {
        if !Self::in_domain(f) {
            None
        } else {
            Some(Self((f * (1_i64 << Self::SHIFT) as f64) as i64))
        }
    }
    /// panics if `f` is outside the domain
    pub(crate) fn from_f64(f: f64) -> Self {
        Self::try_from_f64(f).expect("out of domain")
    }
    pub(crate) fn into_f64(self) -> f64 {
        f64::from(self)
    }
    // pub(crate) fn try_from_f32(f: f32) -> Option<Self> {
    //     Self::try_from_f64(f as f64)
    // }
    // pub(crate) fn into_f32(self) -> f32 {
    //     f32::from(self)
    // }

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

    // TODO: all these ops don't use the fact that it's just a shift
    // but idk how to do the overflow checks myself

    /// computes self * 2, returns `None` if it would overflow
    pub(crate) fn mul2_checked(self) -> Option<Self> {
        self.0.checked_mul(2).map(Self)
    }
    /// computes self * 2, panics on overflow
    pub(crate) fn mul2(self) -> Self {
        self.mul2_checked().expect("overflow in mul2")
    }
    /// computes self * 2^n, returns `None` if we would overflow
    pub(crate) fn mul2_n_checked(self, n: u32) -> Option<Self> {
        self.0.checked_mul(1i64.checked_shl(n)?).map(Self)
        // for _ in 0..n {
        //     self = self.mul2_checked()?;
        // }
        // Some(self)
    }
    /// computes self * 2^n, panics on overflow
    pub(crate) fn mul2_n(self, n: u32) -> Self {
        self.mul2_n_checked(n).expect("overflow in mul2_n")
    }

    /// computes self / 2, returns `None` if we would lose precision (ie if self is odd)
    pub(crate) fn div2_exact_checked(self) -> Option<Self> {
        if self.0 & 1 != 0 {
            None
        } else {
            Some(Self(self.0 >> 1))
        }
    }
    /// computes self / 2, panics if we would lose precision (ie if self is odd)
    pub(crate) fn div2_exact(self) -> Self {
        self.div2_exact_checked()
            .expect("loss of precision in div2_exact")
    }
    /// computes self / 2^n, returns `None` if we would lose precision
    pub(crate) fn div2_n_exact_checked(self, n: u32) -> Option<Self> {
        if self.0.trailing_zeros() < n {
            None
        } else {
            Some(Self(self.0.checked_shr(n)?))
        }
    }
    /// computes self / 2^n, panics if we would lose precision
    pub(crate) fn div2_n_exact(self, n: u32) -> Self {
        self.div2_n_exact_checked(n)
            .expect("loss of precision in div2_n_exact")
    }

    pub(crate) fn div2_floor(self) -> Self {
        Self(self.0 >> 1)
    }
    pub(crate) fn div2_n_floor(self, n: u32) -> Self {
        Self(self.0.checked_shr(n).unwrap())
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
    // TODO: do this better
    pub(crate) fn mul_f64(self, f: f64) -> Self {
        self.mul(f.into())
    }
    pub(crate) fn div_f64(self, f: f64) -> Self {
        self.mul(f.recip().into())
    }
    // pub(crate) fn mul_f32(self, f: f32) -> Self {
    //     self.mul(f.into())
    // }
    // pub(crate) fn div_f32(self, f: f32) -> Self {
    //     self.mul(f.recip().into())
    // }

    /// returns `None` if the length is zero
    /// TODO: returns `None` if it can't be represented as Fixed
    pub(crate) fn normalized(real: Real, imag: Imag) -> Option<(Real, Imag)> {
        let length = (real.into_f64() * real.into_f64() + imag.into_f64() * imag.into_f64()).sqrt();
        if length == 0.0 {
            None
        } else {
            Some((real.div_f64(length), imag.div_f64(length)))
        }
    }
}

impl fmt::Display for Fixed {
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
// impl From<f64> for Fixed {
//     #[track_caller]
//     fn from(f: f64) -> Self {
//         Self::try_from_f64(f).unwrap()
//     }
// }
// impl From<Fixed> for f32 {
//     fn from(f: Fixed) -> Self {
//         f64::from(f) as f32
//     }
// }
// impl From<f32> for Fixed {
//     #[track_caller]
//     fn from(f: f32) -> Self {
//         Self::try_from_f32(f).unwrap()
//     }
// }
// impl TryFrom<f64> for Fixed {
//     type Error = &'static str;

//     fn try_from(f: f64) -> Result<Self, Self::Error> {
//         Self::try_from_f64(f).ok_or("out of domain")
//     }
// }
impl From<f64> for Fixed {
    #[track_caller]
    fn from(f: f64) -> Self {
        Self::try_from_f64(f).expect("out of domain")
    }
}

impl ops::Add for Fixed {
    type Output = Self;

    #[track_caller]
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0.checked_add(rhs.0).expect("overflow in add"))
    }
}
impl ops::AddAssign for Fixed {
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0.checked_add(rhs.0).expect("overflow in add_assign");
    }
}
impl ops::Sub for Fixed {
    type Output = Self;

    #[track_caller]
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0.checked_sub(rhs.0).expect("overflow in sub"))
    }
}
impl ops::SubAssign for Fixed {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = self.0.checked_sub(rhs.0).expect("overflow in sub_assign");
    }
}
impl ops::Neg for Fixed {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(self.0.checked_neg().expect("overflow in neg"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero() {
        assert_eq!(Fixed::ZERO, Fixed::from_f64(0.0));
    }

    #[test]
    fn test_one() {
        assert_eq!(Fixed(1_i64 << Fixed::SHIFT), Fixed::from_f64(1.0));
        assert_eq!(Fixed(1_i64 << Fixed::SHIFT), Fixed::ONE);
    }

    #[test]
    fn test_domain_neg() {
        assert_eq!(Fixed::from_f64(-Fixed::DOMAIN_RADIUS), Fixed::MIN);
        assert_ne!(Fixed::from_f64(-Fixed::DOMAIN_RADIUS / 32.0), Fixed::MIN);
        assert_ne!(Fixed::from_f64(-Fixed::DOMAIN_RADIUS / 16.0), Fixed::MIN);
        assert_ne!(Fixed::from_f64(-Fixed::DOMAIN_RADIUS / 8.0), Fixed::MIN);
        assert_ne!(Fixed::from_f64(-Fixed::DOMAIN_RADIUS / 4.0), Fixed::MIN);
        assert_ne!(Fixed::from_f64(-Fixed::DOMAIN_RADIUS / 2.0), Fixed::MIN);
        assert_ne!(Fixed::from_f64(-Fixed::DOMAIN_RADIUS / 1.5), Fixed::MIN);
        assert_ne!(Fixed::from_f64(-Fixed::DOMAIN_RADIUS / 3.0), Fixed::MIN);
        assert_eq!(Fixed::try_from_f64(-Fixed::DOMAIN_RADIUS * 2.0), None);
    }

    #[test]
    #[should_panic]
    fn test_domain_very_neg() {
        assert_ne!(Fixed::from_f64(-Fixed::DOMAIN_RADIUS * 2.0), Fixed::MIN);
    }

    #[test]
    fn test_domain_pos() {
        assert_ne!(Fixed::try_from_f64(Fixed::DOMAIN_RADIUS / 32.0), None);
        assert_ne!(Fixed::try_from_f64(Fixed::DOMAIN_RADIUS / 16.0), None);
        assert_ne!(Fixed::try_from_f64(Fixed::DOMAIN_RADIUS / 8.0), None);
        assert_ne!(Fixed::try_from_f64(Fixed::DOMAIN_RADIUS / 4.0), None);
        assert_ne!(Fixed::try_from_f64(Fixed::DOMAIN_RADIUS / 2.0), None);
        assert_ne!(Fixed::try_from_f64(Fixed::DOMAIN_RADIUS / 1.5), None);
        assert_ne!(Fixed::try_from_f64(Fixed::DOMAIN_RADIUS / 3.0), None);
    }

    #[test]
    #[should_panic]
    fn test_domain_very_pos() {
        assert_eq!(Fixed::from_f64(Fixed::DOMAIN_RADIUS), Fixed::MIN);
    }

    #[test]
    fn test_identity_f64_fixed() {
        for f in [
            20.601, 20.617, -3.980, -22.092, -21.047, -19.458, 30.177, 20.944, 14.705, -16.787,
            20.603, -24.559, -18.584, -16.767, -6.018, -17.405, -15.284, 16.983, 22.734, -21.853,
            -27.686, -1.317, 17.825, -2.914, -18.354, -14.570, 12.935, 3.635, -9.842, -8.893,
        ] {
            let actual = f64::from(Fixed::from_f64(f));
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
        .map(Fixed::from_f64)
        {
            let actual = Fixed::from_f64(f64::from(f));
            assert_eq!(f, actual);
        }
    }
}
