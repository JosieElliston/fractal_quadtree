use super::{Domain, fixed::*};

#[derive(Debug, Clone, Copy, PartialEq)]
// #[repr(align(32))]
pub(crate) struct Window {
    real_lo: Real,
    real_hi: Real,
    imag_lo: Imag,
    imag_hi: Imag,
}
impl Window {
    /// fails if the window would be empty,
    /// ie if it would have zero width or height.
    /// also fails if the window is too big, to avoid later overflow issues.
    pub(crate) fn try_from_lo_hi(
        real_lo: Real,
        real_hi: Real,
        imag_lo: Imag,
        imag_hi: Imag,
    ) -> Option<Self> {
        if !(real_lo < real_hi && imag_lo < imag_hi) {
            return None;
        }
        let _ = real_hi.add_checked(real_lo)?;
        let _ = imag_hi.add_checked(imag_lo)?;
        // if !(Fixed::in_domain(2.0 * real_lo.into_f64())
        //     && Fixed::in_domain(2.0 * real_hi.into_f64())
        //     && Fixed::in_domain(2.0 * imag_lo.into_f64())
        //     && Fixed::in_domain(2.0 * imag_hi.into_f64()))
        // {
        //     return None;
        // }
        Some(Self {
            real_lo,
            real_hi,
            imag_lo,
            imag_hi,
        })
    }
    /// panics if the window would be empty,
    /// including if it would have zero width or height
    pub(crate) fn from_lo_hi(real_lo: Real, real_hi: Real, imag_lo: Imag, imag_hi: Imag) -> Self {
        Self::try_from_lo_hi(real_lo, real_hi, imag_lo, imag_hi).unwrap()
    }
    /// fails if the window would be empty,
    /// ie if it would have zero width or height.
    /// also fails if the window is too big, to avoid overflow issues.
    pub(crate) fn from_mid_rad(
        real_mid: Real,
        imag_mid: Imag,
        real_rad: Real,
        imag_rad: Imag,
    ) -> Option<Self> {
        assert!(real_rad > Fixed::ZERO);
        assert!(imag_rad > Fixed::ZERO);
        let real_lo = real_mid.sub_checked(real_rad)?;
        let real_hi = real_mid.add_checked(real_rad)?;
        let imag_lo = imag_mid.sub_checked(imag_rad)?;
        let imag_hi = imag_mid.add_checked(imag_rad)?;
        Self::try_from_lo_hi(real_lo, real_hi, imag_lo, imag_hi)
    }
    // /// panics if the diameter is too small
    // pub(crate) fn from_mid_diam(
    //     real_mid: Real,
    //     imag_mid: Imag,
    //     real_diam: Real,
    //     imag_diam: Imag,
    // ) -> Self {
    //     assert!(real_diam > Fixed::ZERO);
    //     assert!(imag_diam > Fixed::ZERO);
    //     let real_rad = real_diam.div2_floor();
    //     let imag_rad = imag_diam.div2_floor();
    //     Self::from_mid_rad(real_mid, imag_mid, real_rad, imag_rad)
    // }

    pub(crate) fn real_lo(self) -> Real {
        self.real_lo
    }
    pub(crate) fn real_hi(self) -> Real {
        self.real_hi
    }
    pub(crate) fn real_mid(self) -> Real {
        (self.real_hi + self.real_lo).div2_floor()
    }
    pub(crate) fn real_rad(self) -> Real {
        (self.real_hi - self.real_lo).div2_floor()
    }

    pub(crate) fn imag_lo(self) -> Imag {
        self.imag_lo
    }
    pub(crate) fn imag_hi(self) -> Imag {
        self.imag_hi
    }
    pub(crate) fn imag_rad(self) -> Imag {
        (self.imag_hi - self.imag_lo).div2_floor()
    }
    pub(crate) fn imag_mid(self) -> Imag {
        (self.imag_hi + self.imag_lo).div2_floor()
    }

    pub(crate) fn mid(self) -> (Real, Imag) {
        (self.real_mid(), self.imag_mid())
    }

    // pub(crate) fn area(self) -> f32 {
    //     (self.real_hi - self.real_lo) * (self.imag_hi - self.imag_lo)
    // }

    pub(crate) fn intersect(self, other: impl Into<Self>) -> Option<Self> {
        let other = other.into();
        let real_lo = Fixed::max(self.real_lo, other.real_lo);
        let real_hi = Fixed::min(self.real_hi, other.real_hi);
        let imag_lo = Fixed::max(self.imag_lo, other.imag_lo);
        let imag_hi = Fixed::min(self.imag_hi, other.imag_hi);
        Self::try_from_lo_hi(real_lo, real_hi, imag_lo, imag_hi)
    }

    pub(crate) fn overlaps(self, other: impl Into<Self>) -> bool {
        let other = other.into();
        let real_lo = Fixed::max(self.real_lo, other.real_lo);
        let real_hi = Fixed::min(self.real_hi, other.real_hi);
        let imag_lo = Fixed::max(self.imag_lo, other.imag_lo);
        let imag_hi = Fixed::min(self.imag_hi, other.imag_hi);
        real_lo <= real_hi && imag_lo <= imag_hi
    }

    pub(crate) fn contains_point(self, (real, imag): (Real, Imag)) -> bool {
        (self.real_lo..=self.real_hi).contains(&real)
            && (self.imag_lo..=self.imag_hi).contains(&imag)
    }

    // fn contains(self, other: impl Into<Self>) -> bool {
    //     let other = other.into();
    //     self.intersect(other) == Some(other)
    // }

    // pub(crate) fn grid(
    //     self,
    //     width: usize,
    //     height: usize,
    // ) -> impl Iterator<Item = impl Iterator<Item = Self>> {
    //     (0..height).map(move |row| {
    //         let imag = Fixed::lerp(
    //             self.imag_lo(),
    //             self.imag_hi(),
    //             1.0 - row as f64 / height as f64,
    //         );
    //         let imag_next = Fixed::lerp(
    //             self.imag_lo(),
    //             self.imag_hi(),
    //             1.0 - (row + 1) as f64 / height as f64,
    //         );
    //         (0..width).map(move |col| {
    //             let real = Fixed::lerp(self.real_lo(), self.real_hi(), col as f64 / width as f64);
    //             let real_next = Fixed::lerp(
    //                 self.real_lo(),
    //                 self.real_hi(),
    //                 (col + 1) as f64 / width as f64,
    //             );
    //             Self::from_lo_hi(real, imag, real_next, imag_next)
    //         })
    //     })
    // }

    /// returns an iterator over the centers of rectangles of a width by height grid
    /// so each point will be strictly inside the window
    /// and the average of the points will be the center of the window
    pub(crate) fn grid_centers(
        self,
        width: usize,
        height: usize,
    ) -> impl Iterator<Item = impl Iterator<Item = (Real, Imag)>> {
        (0..height).map(move |row| {
            let imag = Fixed::lerp(
                self.imag_lo(),
                self.imag_hi(),
                1.0 - (row as f64 + 0.5) / height as f64,
            );
            (0..width).map(move |col| {
                let real = Fixed::lerp(
                    self.real_lo(),
                    self.real_hi(),
                    (col as f64 + 0.5) / width as f64,
                );
                (real, imag)
            })
        })
    }
}
// impl From<Square> for Window {
//     fn from(value: Square) -> Self {
//         Window {
//             real_lo: value.real_lo(),
//             real_hi: value.real_hi(),
//             imag_lo: value.imag_lo(),
//             imag_hi: value.imag_hi(),
//         }
//     }
// }
impl From<Domain> for Window {
    fn from(dom: Domain) -> Self {
        Window {
            real_lo: dom.real_lo(),
            real_hi: dom.real_hi(),
            imag_lo: dom.imag_lo(),
            imag_hi: dom.imag_hi(),
        }
    }
}
// impl PartialOrd for Window {
//     fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
//         if self == other {
//             return Some(Ordering::Equal);
//         }
//         if other.real_lo <= self.real_lo
//             && self.real_hi <= other.real_hi
//             && other.imag_lo <= self.imag_lo
//             && self.imag_hi <= other.imag_hi
//         {
//             return Some(Ordering::Less);
//         }
//         if self.real_lo <= other.real_lo
//             && other.real_hi <= self.real_hi
//             && self.imag_lo <= other.imag_lo
//             && other.imag_hi <= self.imag_hi
//         {
//             return Some(Ordering::Greater);
//         }
//         None
//     }
// }
