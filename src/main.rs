mod elevation_reader;
mod tile_encoder;

use elevation_reader::ElevationReader;
use tile_encoder::TileEncoder;
use oxigdal_core::buffer::RasterBuffer;
use image::{ImageBuffer, Luma};
use oxigdal_algorithms::raster::swiss_hillshade;
use contour::ContourBuilder;
use futures::StreamExt;

const TILE_SIZE: usize = 512;
const THRESHOLDS:  [f64; 4] = [96.0, 112.0, 140.0, 256.0];

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
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = "planet.pmtiles";
    let reader = ElevationReader::new(file, TILE_SIZE).await?;
    let mut encoder = TileEncoder::new("example.pmtiles")?;

    let mut tiles = Box::pin(reader.iter_tiles());
    while let Some(tile_result) = tiles.next().await {
        let tile = tile_result?;
        let elevation = reader.get(tile).await?;
        let mut hillshade = swiss_hillshade(&elevation, 1.0, 30.0).unwrap();
        // save_buffer_as_png(&hillshade, "hillshade.png");

        for i in 0..TILE_SIZE {
            for j in 0..TILE_SIZE {
                hillshade.set_pixel(i as u64, j as u64, 255.0 - hillshade.get_pixel(i as u64, j as u64).unwrap());
            }
        }
        
        let c = ContourBuilder::new(TILE_SIZE, TILE_SIZE, true);
        let bands = c.isobands(hillshade.as_slice().unwrap(), &THRESHOLDS).unwrap();

        encoder.encode(TILE_SIZE, tile, &bands)?;
    }

    encoder.finalize()?;
    
    Ok(())
}