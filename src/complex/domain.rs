use std::fmt;

use crate::tree::Offset;

use super::fixed::*;

/// this is not just any square,
/// a `Domain` must be derived by splitting the default domain into four children,
/// which ensures no rounding occurs.
///
/// `[real_mid - rad, real_mid + rad) x [imag_mid - rad, imag_mid + rad)`
///
/// must have that rad > 0.
// TODO: possibly we can have rad >= 0, but whatever
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
pub(crate) struct Domain {
    real_mid: Real,
    imag_mid: Imag,
    rad: Fixed,
}
impl Default for Domain {
    /// [-4, 4] x [-4, 4]
    /// or maybe (-4, 4) x (-4, 4)
    fn default() -> Self {
        Self {
            real_mid: Fixed::ZERO,
            imag_mid: Fixed::ZERO,
            rad: Fixed::ONE.mul2().mul2(),
        }
    }
}
impl Domain {
    pub(crate) fn uninit() -> Self {
        unsafe { std::mem::zeroed() }
    }

    /// splits the domain into four equal squares.
    ///
    /// returns `None` if the radius would be too small.
    ///
    /// in the order:
    /// ```
    /// 0 1
    /// 2 3
    /// ```
    pub(crate) fn split(self) -> Option<[Self; 4]> {
        let rad = self.rad().div2_exact_checked()?;
        if rad <= Fixed::ZERO {
            return None;
        }
        Some([
            Self {
                real_mid: self.real_mid() - rad,
                imag_mid: self.imag_mid() + rad,
                rad,
            },
            Self {
                real_mid: self.real_mid() + rad,
                imag_mid: self.imag_mid() + rad,
                rad,
            },
            Self {
                real_mid: self.real_mid() - rad,
                imag_mid: self.imag_mid() - rad,
                rad,
            },
            Self {
                real_mid: self.real_mid() + rad,
                imag_mid: self.imag_mid() - rad,
                rad,
            },
        ])
    }

    pub(crate) fn real_lo(self) -> Real {
        self.real_mid - self.rad
    }
    pub(crate) fn real_hi(self) -> Real {
        self.real_mid + self.rad
    }
    pub(crate) fn real_mid(self) -> Real {
        self.real_mid
    }

    pub(crate) fn imag_lo(self) -> Imag {
        self.imag_mid - self.rad
    }
    pub(crate) fn imag_hi(self) -> Imag {
        self.imag_mid + self.rad
    }
    pub(crate) fn imag_mid(self) -> Imag {
        self.imag_mid
    }

    pub(crate) fn mid(self) -> (Real, Imag) {
        (self.real_mid(), self.imag_mid())
    }
    pub(crate) fn rad(self) -> Fixed {
        self.rad
    }

    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn contains_point(self, (real, imag): (Real, Imag)) -> bool {
        (self.real_lo()..self.real_hi()).contains(&real)
            && (self.imag_lo()..self.imag_hi()).contains(&imag)
        // (self.real_lo()..=self.real_hi()).contains(&real)
        //     && (self.imag_lo()..=self.imag_hi()).contains(&imag)
        // (self.real_mid() - real).abs() <= self.rad() && (self.imag_mid() - imag).abs() <= self.rad()
        // f32::max(
        //     (self.real_mid() - real).abs(),
        //     (self.imag_mid() - imag).abs(),
        // ) <= self.rad()
    }

    /// the point may not be inside the domain.
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn quadrant_offset_containing(&self, (real, imag): (Real, Imag)) -> Offset {
        (if real < self.real_mid() { 0 } else { 1 }) + (if imag >= self.imag_mid() { 0 } else { 2 })
    }

    /// the point must be inside the domain.
    // TODO: should this fail if the child would be too small?
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn child_offset_containing(&self, (real, imag): (Real, Imag)) -> Offset {
        debug_assert!(self.contains_point((real, imag)));

        let ret = self.quadrant_offset_containing((real, imag));

        #[cfg(debug_assertions)]
        if let Some(children) = self.split() {
            debug_assert!(children[ret].contains_point((real, imag)))
        }

        ret
    }
}
impl fmt::Display for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Domain {{ real_mid: {}, imag_mid: {}, rad: {} }}",
            self.real_mid(),
            self.imag_mid(),
            self.rad()
        )
    }
}
