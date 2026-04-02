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
// /// with fancy dynamic radius based on zoom
// /// ref how i did it in delaunay
// fn draw_complex_filled_circle() {
//     painter.circle_filled(
//         camera_map.cam_to_egui(point),
//         // so if you're zoomed out, points don't cover everything
//         (camera_map.cam_to_egui_x(0.001) - camera_map.cam_to_egui_x(0.0)).min(3.0),
//         Color32::from_rgb(200, 200, 200),
//     );
// }