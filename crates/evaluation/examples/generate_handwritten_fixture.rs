use std::path::Path;

use image::{imageops, ImageReader, Rgb, RgbImage};

const OUTPUT: &str = "evaluation/corpora/assets/handwritten-formula.png";
const ORIENTATION_SOURCE: &str = "tests/fixtures/openocr/text-en.png";
const ORIENTATION_OUTPUT: &str = "evaluation/corpora/assets/orientation.png";

fn main() {
    let mut image = RgbImage::from_pixel(640, 240, Rgb([255, 255, 255]));
    let strokes: &[&[(f32, f32)]] = &[
        &[(70.0, 82.0), (92.0, 111.0), (112.0, 143.0), (137.0, 174.0)],
        &[(139.0, 78.0), (119.0, 109.0), (96.0, 143.0), (72.0, 177.0)],
        &[(151.0, 61.0), (162.0, 46.0), (184.0, 45.0), (194.0, 58.0)],
        &[(194.0, 58.0), (201.0, 69.0), (187.0, 80.0), (171.0, 91.0)],
        &[(171.0, 91.0), (180.0, 91.0), (190.0, 91.0), (199.0, 91.0)],
        &[
            (226.0, 126.0),
            (246.0, 126.0),
            (266.0, 125.0),
            (286.0, 125.0),
        ],
        &[
            (255.0, 95.0),
            (256.0, 115.0),
            (256.0, 136.0),
            (257.0, 156.0),
        ],
        &[
            (320.0, 81.0),
            (337.0, 111.0),
            (358.0, 143.0),
            (381.0, 174.0),
        ],
        &[
            (385.0, 78.0),
            (365.0, 111.0),
            (348.0, 143.0),
            (329.0, 199.0),
        ],
        &[
            (329.0, 199.0),
            (323.0, 215.0),
            (311.0, 219.0),
            (299.0, 213.0),
        ],
        &[(396.0, 61.0), (407.0, 46.0), (429.0, 45.0), (439.0, 58.0)],
        &[(439.0, 58.0), (446.0, 69.0), (432.0, 80.0), (416.0, 91.0)],
        &[(416.0, 91.0), (425.0, 91.0), (435.0, 91.0), (444.0, 91.0)],
        &[
            (474.0, 110.0),
            (497.0, 110.0),
            (520.0, 109.0),
            (543.0, 109.0),
        ],
        &[
            (475.0, 144.0),
            (497.0, 144.0),
            (520.0, 143.0),
            (542.0, 143.0),
        ],
        &[(584.0, 87.0), (594.0, 82.0), (602.0, 74.0), (608.0, 67.0)],
        &[
            (608.0, 67.0),
            (607.0, 103.0),
            (607.0, 139.0),
            (606.0, 174.0),
        ],
        &[
            (587.0, 174.0),
            (599.0, 174.0),
            (611.0, 174.0),
            (623.0, 174.0),
        ],
    ];
    for stroke in strokes {
        draw_cubic(&mut image, stroke, 4);
    }
    image.save(Path::new(OUTPUT)).expect("save fixture");
    println!("wrote {OUTPUT}");

    let source = ImageReader::open(ORIENTATION_SOURCE)
        .expect("open orientation source")
        .decode()
        .expect("decode orientation source")
        .to_rgb8();
    imageops::rotate90(&source)
        .save(ORIENTATION_OUTPUT)
        .expect("save orientation fixture");
    println!("wrote {ORIENTATION_OUTPUT}");
}

fn draw_cubic(image: &mut RgbImage, points: &[(f32, f32)], radius: i32) {
    assert_eq!(points.len(), 4);
    for step in 0..=160 {
        let t = step as f32 / 160.0;
        let inverse = 1.0 - t;
        let x = inverse.powi(3) * points[0].0
            + 3.0 * inverse.powi(2) * t * points[1].0
            + 3.0 * inverse * t.powi(2) * points[2].0
            + t.powi(3) * points[3].0;
        let y = inverse.powi(3) * points[0].1
            + 3.0 * inverse.powi(2) * t * points[1].1
            + 3.0 * inverse * t.powi(2) * points[2].1
            + t.powi(3) * points[3].1;
        draw_disc(image, x.round() as i32, y.round() as i32, radius);
    }
}

fn draw_disc(image: &mut RgbImage, center_x: i32, center_y: i32, radius: i32) {
    for y in -radius..=radius {
        for x in -radius..=radius {
            if x * x + y * y > radius * radius {
                continue;
            }
            let pixel_x = center_x + x;
            let pixel_y = center_y + y;
            if let Some(pixel) = image.get_pixel_mut_checked(pixel_x as u32, pixel_y as u32) {
                *pixel = Rgb([23, 23, 23]);
            }
        }
    }
}
