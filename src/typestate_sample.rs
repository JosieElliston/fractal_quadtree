// use crate::fixed::*;

// trait Derivative {}
// struct NoDerivative;
// impl Derivative for NoDerivative {}
// struct YesDerivative {
//     zp_real: Real,
//     zp_imag: Imag,
// };
// impl Derivative for YesDerivative {}
// /// [typestate](https://cliffle.com/blog/rust-typestate/)
// /// TODO: is it ok to put trait bounds on the struct here?
// struct Sample<D: Derivative> {
//     depth: u32,
//     z_real: Real,
//     z_imag: Imag,
//     dz: D,
// }
// impl Sample {
//     fn new(depth: u32, z_real: Real, z_imag: Imag) -> Sample<NoDerivative> {
//         Sample {
//             depth,
//             z_real,
//             z_imag,
//             dz: NoDerivative,
//         }
//     }
//     fn with_derivative(self, zp_real: Real, zp_imag: Imag) -> Sample<YesDerivative> {
//         Sample {
//             depth: self.depth,
//             z_real: self.z_real,
//             z_imag: self.z_imag,
//             dz: YesDerivative { zp_real, zp_imag },
//         }
//     }

//     fn smooth_depth(&self) ->  {}
//     fn distance_estimate(&self) ->  {}
// }
