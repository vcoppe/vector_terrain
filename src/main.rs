mod elevation_reader;

use elevation_reader::ElevationReader;
use pmtiles::TileCoord;
use oxigdal_terrain::derivatives::hillshade::hillshade_traditional;
use ndarray::Array2;
use image::{ImageBuffer, Luma};

fn save_array_as_png(data: &Array2<u8>, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (height, width) = data.dim();
    
    // Convert Array2 to Vec in row-major order
    let pixels: Vec<u8> = data
        .iter()
        .copied()
        .collect();
    
    // Create ImageBuffer (Luma = grayscale)
    let img = ImageBuffer::<Luma<u8>, Vec<u8>>::from_raw(width as u32, height as u32, pixels)
        .ok_or("Failed to create image buffer")?;
    
    img.save(filename)?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let file = "planet.pmtiles";
    let tile_size = 512;
    let reader = ElevationReader::new(file, tile_size).await;
    
    let coord = TileCoord::new(12, 2078, 1554).unwrap();
    let elevation = reader.get(coord).await;
    let hillshade = hillshade_traditional(&elevation, 1.0, 215.0, 45.0, 1.0, None).unwrap();
    save_array_as_png(&hillshade, "hillshade.png");
    println!("{:?}", hillshade);
}