use super::fixed::*;

/// must have that rad > 0
/// this is not any square, a `Domain` comes from splitting the default domain into four children
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

    // /// returns `None` if the radius is too small
    // /// the caller must ensure that the stuff will maintain the invariant
    // fn from_mid_rad(real_mid: Real, imag_mid: Imag, rad: Fixed) -> Option<Self> {
    //     if !(rad > Fixed::ZERO) {
    //         return None;
    //     }
    //     Some(Self {
    //         real_mid,
    //         imag_mid,
    //         rad,
    //     })
    // }

    // /// returns `None` if the radius would be too small
    // /// panics if real_hi - real_lo != imag_hi - imag_lo
    // pub(crate) fn from_lo_hi(
    //     real_lo: ExactReal,
    //     real_hi: ExactReal,
    //     imag_lo: ExactImag,
    //     imag_hi: ExactImag,
    // ) -> Option<Self> {
    //     if !(real_lo < real_hi && imag_lo < imag_hi) {
    //         return None;
    //     }
    //     if real_hi - real_lo != imag_hi - imag_lo {
    //         return None;
    //     }
    //     Some(Self {
    //         real_mid: (real_lo + real_hi).div2_exact_checked()?,
    //         imag_mid: (imag_lo + imag_hi).div2_exact_checked()?,
    //         rad: (real_hi - real_lo).div2_exact_checked()?,
    //     })
    // }

    /// returns `None` if the radius would be too small
    ///
    /// 0 1
    ///
    /// 2 3
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

    // pub(crate) fn contains_point(self, (real, imag): (ExactReal, ExactImag)) -> bool {
    // pub(crate) fn contains_point(self, (real, imag): (Real, Imag)) -> bool {
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

    /// the point must be inside the domain
    #[cfg_attr(feature = "profiling", inline(never))]
    pub(crate) fn child_offset_containing(&self, (real, imag): (Real, Imag)) -> usize {
        debug_assert!(self.contains_point((real, imag)));
        (if real < self.real_mid() { 0 } else { 1 }) + (if imag >= self.imag_mid() { 0 } else { 2 })
    }
}
