mod elevation_reader;
mod tile_encoder;

use elevation_reader::ElevationReader;
use tile_encoder::TileEncoder;
use pmtiles::TileCoord;
use oxigdal_core::buffer::RasterBuffer;
use image::{ImageBuffer, Luma};
use oxigdal_algorithms::raster::swiss_hillshade;
use contour::ContourBuilder;

const TILE_SIZE: usize = 512;
// const THRESHOLDS:  [f64; 5] = [0.0, 96.0, 112.0, 140.0, 256.0];
const THRESHOLDS:  [f64; 4] = [96.0, 112.0, 140.0, 256.0];
const COLORS:  [f64; 5] = [256.0, 256.0, 224.0, 160.0, 128.0];
const CLASS_NAMES:  [&str; 3] = ["0", "1", "2"];

fn get_class(value: f64) -> f64 {
    for (i, t) in THRESHOLDS.iter().enumerate() {
        if value <= *t {
            return COLORS[i];
        }
    }
    0.0
}

fn save_buffer_as_png(data: &RasterBuffer, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut pixels = vec![0; TILE_SIZE * TILE_SIZE];
    for i in 0..TILE_SIZE {
        for j in 0..TILE_SIZE {
            pixels[i * TILE_SIZE + j] = data.get_pixel(i as u64, j as u64).unwrap() as u8;
        }
    }
    as_png(pixels, filename)
}

fn save_array_as_png(data: &Vec<f64>, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut pixels = vec![0; TILE_SIZE * TILE_SIZE];
    for i in 0..TILE_SIZE {
        for j in 0..TILE_SIZE {
            pixels[i * TILE_SIZE + j] = data[i * TILE_SIZE + j] as u8;
        }
    }
    as_png(pixels, filename)
}

fn as_png(pixels: Vec<u8>, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    let img = ImageBuffer::<Luma<u8>, Vec<u8>>::from_raw(TILE_SIZE as u32, TILE_SIZE as u32, pixels)
        .ok_or("Failed to create image buffer")?;
    img.save(filename)?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let file = "planet.pmtiles";
    let reader = ElevationReader::new(file, TILE_SIZE).await;
    
    let coord = TileCoord::new(12, 2078, 1554).unwrap();
    let elevation = reader.get(coord).await;
    let mut hillshade = swiss_hillshade(&elevation, 1.0, 30.0).unwrap();
    save_buffer_as_png(&hillshade, "hillshade.png");

    for i in 0..TILE_SIZE {
        for j in 0..TILE_SIZE {
            hillshade.set_pixel(i as u64, j as u64, 255.0 - hillshade.get_pixel(i as u64, j as u64).unwrap());
        }
    }

    // let mut classes = vec![0.0; TILE_SIZE * TILE_SIZE];
    // for i in 0..TILE_SIZE {
    //     for j in 0..TILE_SIZE {
    //         classes[i * TILE_SIZE + j] = get_class(hillshade.get_pixel(i as u64, j as u64).unwrap());
    //     }
    // }
    // save_array_as_png(&classes, "classes.png");
    
    let c = ContourBuilder::new(TILE_SIZE, TILE_SIZE, false);
    let bands = c.isobands(hillshade.as_slice().unwrap(), &THRESHOLDS).unwrap();

    TileEncoder::encode(TILE_SIZE, coord, &bands, &CLASS_NAMES);
}