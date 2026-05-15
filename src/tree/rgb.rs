use std::{fmt, num::NonZeroU32};

use egui::Color32;

/// basically a [`egui::Color32`] with max alpha,
/// so we can use `Option` niche optimization.
/// layout is 0xFFbbggrr, ie little endian [r, g, b, 255].
/// we could allow any nonzero alpha, but i don't use this.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, bytemuck::NoUninit)]
pub(super) struct Rgb(NonZeroU32);
unsafe impl bytemuck::ZeroableInOption for Rgb {}
unsafe impl bytemuck::PodInOption for Rgb {}
impl Rgb {
    pub(super) fn uninit() -> Self {
        Self(NonZeroU32::new(0xABCDEFAB).unwrap())
    }

    pub(super) const fn new(r: u8, g: u8, b: u8) -> Self {
        let arr = [r, g, b, 255];
        let value = u32::from_le_bytes(arr);
        Rgb(NonZeroU32::new(value).unwrap())
    }
}
impl fmt::Debug for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b, a] = self.0.get().to_le_bytes();
        debug_assert_eq!(a, 255);
        f.debug_struct("RGB")
            .field("r", &r)
            .field("g", &g)
            .field("b", &b)
            .finish()
    }
}
impl TryFrom<Color32> for Rgb {
    type Error = &'static str;

    fn try_from(value: Color32) -> Result<Self, Self::Error> {
        if value.a() != 255 {
            return Err("alpha is not 255");
        }
        Ok(Self::new(value.r(), value.g(), value.b()))
    }
}
impl From<Rgb> for Color32 {
    fn from(value: Rgb) -> Self {
        let [r, g, b, a] = value.0.get().to_le_bytes();
        debug_assert_eq!(a, 255);
        Color32::from_rgb(r, g, b)
    }
}

// /// represents the average of `count` colors
// #[derive(Debug, Default, Clone)]
// #[repr(align(32))]
// pub(crate) struct ColorBuilder {
//     count: u32,
//     r: u32,
//     g: u32,
//     b: u32,
// }
// impl ColorBuilder {
//     pub(crate) fn build(self) -> Option<Color32> {
//         if self.count == 0 {
//             None
//         } else {
//             Some(Color32::from_rgb(
//                 (self.r / self.count) as u8,
//                 (self.g / self.count) as u8,
//                 (self.b / self.count) as u8,
//             ))
//         }
//     }
// }
// impl From<Color32> for ColorBuilder {
//     fn from(value: Color32) -> Self {
//         Self {
//             count: 1,
//             r: value.r() as _,
//             g: value.g() as _,
//             b: value.b() as _,
//         }
//     }
// }
// impl AddAssign<ColorBuilder> for ColorBuilder {
//     fn add_assign(&mut self, rhs: ColorBuilder) {
//         self.count += rhs.count;
//         self.r += rhs.r;
//         self.g += rhs.g;
//         self.b += rhs.b;
//     }
// }
// impl Add<ColorBuilder> for ColorBuilder {
//     type Output = ColorBuilder;

//     fn add(self, rhs: ColorBuilder) -> ColorBuilder {
//         let mut result = self;
//         result += rhs;
//         result
//     }
// }
// impl Sum for ColorBuilder {
//     fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
//         let mut ret = Self::default();
//         for c in iter {
//             ret += c;
//         }
//         ret
//     }
// }
