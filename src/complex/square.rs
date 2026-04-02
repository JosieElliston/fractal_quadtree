use super::fixed::*;

#[derive(Debug, Clone, Copy, PartialEq)]
// #[repr(align(32))]
#[repr(C)]
pub(crate) struct Square {
    // real_lo: f32,
    // imag_lo: f32,
    // real_hi: f32,
    // imag_hi: f32,
    real_mid: Real,
    imag_mid: Imag,
    rad: Fixed,
}
impl Square {
    /// for pixels, where things are imprecise
    pub(crate) fn try_new(
        real_lo: Real,
        real_hi: Real,
        imag_lo: Imag,
        imag_hi: Imag,
    ) -> Option<Self> {
        if !(real_lo <= real_hi && imag_lo <= imag_hi) {
            return None;
        }
        if !(real_lo < real_hi && imag_lo < imag_hi) {
            return None;
        }
        // if !{
        //     let dx = real_hi - real_lo;
        //     let dy = imag_hi - imag_lo;
        //     let diff = dx - dy;
        //     let ratio = dx / dy;
        //     diff.abs() < 1e-4 || (1.0 - ratio).abs() < 1e-4
        // } {
        //     return None;
        // }

        // dbg!(real_lo, real_hi, -real_lo);
        // println!(
        //     "real_lo: {}, real_hi: {}, -real_lo: {}",
        //     real_lo, real_hi, -real_lo
        // );
        // std::hint::black_box(real_hi + (-real_lo));
        // std::hint::black_box(real_hi - real_lo);

        let real_diameter = real_hi - real_lo;
        let imag_diameter = imag_hi - imag_lo;
        if (real_diameter - imag_diameter).abs() > (1e-4).into() {
            return None;
        }

        // Some(Self {
        //     real_lo,
        //     real_hi,
        //     imag_lo,
        //     imag_hi,
        // })
        Some(Self {
            real_mid: (real_lo + real_hi).div2_floor(),
            imag_mid: (imag_lo + imag_hi).div2_floor(),
            // rad: (real_hi - real_lo).div2_floor(),
            rad: (real_diameter + imag_diameter).div2_floor().div2_floor(),
        })
    }

    /// for the tree, where domains are aligned to powers of 2
    pub(crate) fn new_exact(
        real_lo: Real,
        real_hi: Real,
        imag_lo: Imag,
        imag_hi: Imag,
    ) -> Option<Self> {
        if !(real_lo < real_hi && imag_lo < imag_hi) {
            return None;
        }
        if real_hi - real_lo != imag_hi - imag_lo {
            return None;
        }
        Some(Self {
            real_mid: (real_lo + real_hi).div2_exact_checked()?,
            imag_mid: (imag_lo + imag_hi).div2_exact_checked()?,
            rad: (real_hi - real_lo).div2_exact_checked()?,
        })
    }

    pub(crate) fn real_lo(self) -> Real {
        // self.real_lo
        self.real_mid - self.rad
    }
    pub(crate) fn real_hi(self) -> Real {
        // self.real_hi
        self.real_mid + self.rad
    }
    pub(crate) fn real_mid(self) -> Real {
        // (self.real_hi + self.real_lo) / 2.0
        self.real_mid
    }

    pub(crate) fn imag_lo(self) -> Imag {
        // self.imag_lo
        self.imag_mid - self.rad
    }
    pub(crate) fn imag_hi(self) -> Imag {
        // self.imag_hi
        self.imag_mid + self.rad
    }
    pub(crate) fn imag_mid(self) -> Imag {
        // (self.imag_hi + self.imag_lo) / 2.0
        self.imag_mid
    }

    pub(crate) fn mid(self) -> (Real, Imag) {
        (self.real_mid(), self.imag_mid())
    }
    pub(crate) fn rad(self) -> Fixed {
        // (self.real_hi - self.real_lo) / 2.0
        self.rad
    }

    // fn area(self) -> f32 {
    //     (self.real_hi - self.real_lo) * (self.imag_hi - self.imag_lo)
    // }

    // fn contains(self, real: f32, imag: f32) -> bool {
    //     (self.real_lo..=self.real_hi).contains(&real)
    //         && (self.imag_lo..=self.imag_hi).contains(&imag)
    // }
    // #[inline(never)]
    pub(crate) fn contains_point(self, (real, imag): (Real, Imag)) -> bool {
        // (self.real_lo()..=self.real_hi()).contains(&real)
        //     && (self.imag_lo()..=self.imag_hi()).contains(&imag)
        (self.real_mid() - real).abs() <= self.rad() && (self.imag_mid() - imag).abs() <= self.rad()
        // f32::max(
        //     (self.real_mid() - real).abs(),
        //     (self.imag_mid() - imag).abs(),
        // ) <= self.rad()
    }

    // pub(crate) fn approx_contains_point(self, real: f32, imag: f32) -> bool {
    //     // (self.real_lo..=self.real_hi).contains(&real)
    //     //     && (self.imag_lo..=self.imag_hi).contains(&imag)
    //     (self.real_mid() - real).abs() <= self.rad() + 1e-4
    //         && (self.imag_mid() - imag).abs() <= self.rad() + 1e-4
    //     // f32::max(
    //     //     (self.real_mid() - real).abs(),
    //     //     (self.imag_mid() - imag).abs(),
    //     // ) <= self.rad()
    // }

    pub(crate) fn contains_square(self, other: Square) -> bool {
        Fixed::max(
            (self.real_mid() - other.real_mid()).abs(),
            (self.imag_mid() - other.imag_mid()).abs(),
        ) <= self.rad() - other.rad()
    }

    // #[inline(never)]
    pub(crate) fn overlaps(self, other: Self) -> bool {
        ((self.real_mid() - other.real_mid()).abs() <= (self.rad() + other.rad()))
            && ((self.imag_mid() - other.imag_mid()).abs() <= (self.rad() + other.rad()))
        // f32::max(
        //     (self.real_mid() - other.real_mid()).abs(),
        //     (self.imag_mid() - other.imag_mid()).abs(),
        // ) <= self.rad() + other.rad()
    }

    // fn overlaps(self, other: Self) -> bool {
    //     let real_lo = f32::max(self.real_lo(), other.real_lo());
    //     let real_hi = f32::min(self.real_hi(), other.real_hi());
    //     let imag_lo = f32::max(self.imag_lo(), other.imag_lo());
    //     let imag_hi = f32::min(self.imag_hi(), other.imag_hi());
    //     real_lo <= real_hi && imag_lo <= imag_hi
    // }

    // #[inline(never)]
    // fn overlaps(self, other: Self) -> bool {
    //     let self_lo: Simd<f32, 2> = [self.real_lo(), self.imag_lo()].into();
    //     let self_hi: Simd<f32, 2> = [self.real_hi(), self.imag_hi()].into();
    //     let other_lo: Simd<f32, 2> = [other.real_lo(), other.imag_lo()].into();
    //     let other_hi: Simd<f32, 2> = [other.real_hi(), other.imag_hi()].into();
    //     let max = self_lo.simd_max(other_lo);
    //     let min = self_hi.simd_min(other_hi);
    //     max.simd_gt(min) == Mask::from_bitmask(0)
    // }

    // #[inline(never)]
    // fn overlaps(self, other: Self) -> bool {
    //     !(self.real_hi() < other.real_lo()
    //         || other.real_hi() < self.real_lo()
    //         || self.imag_hi() < other.imag_lo()
    //         || other.imag_hi() < self.imag_lo())
    // }
}
// impl PartialOrd for Square {
//     fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
//         std::convert::Into::<Window>::into(*self).partial_cmp(&(*other).into())
//     }
// }
