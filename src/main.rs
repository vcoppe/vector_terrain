mod elevation_reader;
mod tile_encoder;

use elevation_reader::ElevationReader;
use tile_encoder::TileEncoder;
use oxigdal_core::buffer::RasterBuffer;
use oxigdal_core::types::RasterDataType;
use image::{ImageBuffer, Luma};
use oxigdal_algorithms::raster::swiss_hillshade;
use contour::ContourBuilder;
use futures::StreamExt;

const TILE_SIZE: usize = 512;
const PADDING: usize = 16;
const THRESHOLDS:  [f64; 4] = [96.0, 112.0, 140.0, 256.0];

fn save_buffer_as_png(data: &RasterBuffer, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut pixels = vec![0; (data.height() * data.width()) as usize];
    for i in 0..data.height() {
        for j in 0..data.width() {
            pixels[(i * data.width() + j) as usize] = data.get_pixel(i, j)? as u8;
        }
    }
    as_png(pixels, data.width() as u32, filename)
}

// fn save_array_as_png(data: &Vec<f64>, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
//     let mut pixels = vec![0; TILE_SIZE * TILE_SIZE];
//     for i in 0..TILE_SIZE {
//         for j in 0..TILE_SIZE {
//             pixels[i * TILE_SIZE + j] = data[i * TILE_SIZE + j] as u8;
//         }
//     }
//     as_png(pixels, filename)
// }

fn as_png(pixels: Vec<u8>, size: u32, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    let img = ImageBuffer::<Luma<u8>, Vec<u8>>::from_raw(size, size, pixels)
        .ok_or("Failed to create image buffer")?;
    img.save(filename)?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = "planet.pmtiles";
    let reader = ElevationReader::new(file, TILE_SIZE, PADDING).await?;
    let mut encoder = TileEncoder::new("example.pmtiles", TILE_SIZE, PADDING)?;

    let mut elevation = RasterBuffer::zeros((TILE_SIZE + 2 * PADDING) as u64, (TILE_SIZE + 2 * PADDING) as u64, RasterDataType::Float64);

    let mut tiles = Box::pin(reader.iter_tiles());
    while let Some(tile_result) = tiles.next().await {
        let tile = tile_result?;
        reader.fill(&mut elevation, tile, (0, 0)).await?;
        let mut hillshade = swiss_hillshade(&elevation, 1.0, 30.0)?;
        // save_buffer_as_png(&hillshade, "hillshade.png")?;

        for i in 0..hillshade.height() {
            for j in 0..hillshade.width() {
                hillshade.set_pixel(i, j, 255.0 - hillshade.get_pixel(i, j)?)?;
            }
        }
        
        let c = ContourBuilder::new(TILE_SIZE + 2 * PADDING, TILE_SIZE + 2 * PADDING, true);
        let bands = c.isobands(hillshade.as_slice()?, &THRESHOLDS)?;

        encoder.encode(tile, &bands)?;
    }

    encoder.finalize()?;
    
    Ok(())
}

fn process_tile() {

}