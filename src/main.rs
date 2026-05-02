mod elevation_reader;

use elevation_reader::ElevationReader;
use pmtiles::TileCoord;
use oxigdal_core::buffer::RasterBuffer;
use image::{ImageBuffer, Luma};
use oxigdal_core::types::{GeoTransform, BoundingBox};
use oxigdal_algorithms::raster::swiss_hillshade;
use oxigdal_algorithms::raster::polygonize::{polygonize_raster, PolygonizeOptions};
use mercantile::{Tile, bounds};

const TILE_SIZE: usize = 512;

fn save_array_as_png(data: &RasterBuffer, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut pixels = vec![0; TILE_SIZE * TILE_SIZE];
    for i in 0..TILE_SIZE {
        for j in 0..TILE_SIZE {
            pixels[i * TILE_SIZE + j] = data.get_pixel(i as u64, j as u64).unwrap() as u8;
        }
    }
    let img = ImageBuffer::<Luma<u8>, Vec<u8>>::from_raw(TILE_SIZE as u32, TILE_SIZE as u32, pixels)
        .ok_or("Failed to create image buffer")?;
    img.save(filename)?;
    Ok(())
}

fn get_class(hillshade: f64) -> f64 {
    if hillshade <= 128.0 {
        128.0
    } else if hillshade <= 144.0 {
        160.0
    } else if hillshade <= 160.0 {
        224.0
    } else {
        256.0
    }
}

#[tokio::main]
async fn main() {
    let file = "planet.pmtiles";
    let reader = ElevationReader::new(file, TILE_SIZE).await;
    
    let coord = TileCoord::new(12, 2078, 1554).unwrap();
    let elevation = reader.get(coord).await;
    let mut hillshade = swiss_hillshade(&elevation, 1.0, 30.0).unwrap();
    save_array_as_png(&hillshade, "hillshade.png");
    
    for i in 0..TILE_SIZE {
        for j in 0..TILE_SIZE {
            hillshade.set_pixel(i as u64, j as u64, get_class(hillshade.get_pixel(i as u64, j as u64).unwrap()));
        }
    }
    save_array_as_png(&hillshade, "classes.png");

    // Compute bbox
    let tile = Tile::new(coord.x() as i32, coord.y() as i32, coord.z() as i32);
    let bbox = bounds(tile);
    let bbox = BoundingBox::new(bbox.west, bbox.south, bbox.east, bbox.north).unwrap();
    println!("{:?}", bbox);

    let mut opts = PolygonizeOptions::default();
    opts.nodata = Some(256.0);
    opts.transform = Some(GeoTransform::from_bounds(&bbox, TILE_SIZE as u64, TILE_SIZE as u64).unwrap());
    opts.min_area = 100.0;
    let result = polygonize_raster(&hillshade, &opts).unwrap();
    println!("nb polygons {}", result.polygons.len());
}