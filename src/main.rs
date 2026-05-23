mod elevation_reader;
mod tile_encoder;

use clap::Parser;
use contour::ContourBuilder;
use futures::{StreamExt, TryStreamExt};
use image::{ImageBuffer, Luma};
use mercantile::{Tile, ul};
use oxigdal_algorithms::raster::swiss_hillshade;
use oxigdal_core::buffer::RasterBuffer;
use oxigdal_core::types::RasterDataType;
use pmtiles::TileCoord;
use std::{fs::create_dir_all, sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
}};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

use elevation_reader::ElevationReader;
use tile_encoder::TileEncoder;

use crate::elevation_reader::ElevationBounds;

const TILE_SIZE: usize = 512;
const PADDING: usize = 16;
const THRESHOLDS: [f64; 3] = [112.0, 144.0, 176.0];
const COLORS:  [u8; 3] = [224, 160, 128];
const FEET_TO_METER: f64 = 0.3048;

/// A utility for converting a WebP terrain PMTiles file
/// to another PMTiles file with vector hillshading and contour lines
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// the input PMTiles file from Mapterhorn
    #[arg(short, long)]
    input: String,
    /// the output PMTiles file
    #[arg(short, long, default_value_t = String::from("vector_terrain.pmtiles"))]
    output: String,
    /// compute vectorized hillshading
    #[arg(long, default_value_t = false)]
    hillshading: bool,
    /// compute contour lines (meters)
    #[arg(long, default_value_t = false)]
    contours_m: bool,
    /// compute contour lines (feet)
    #[arg(long, default_value_t = false)]
    contours_ft: bool,
    /// ignore tiles below minimum zoom
    #[arg(long, default_value_t = 4)]
    min_zoom: u8,
    /// ignore tiles above maximum zoom
    #[arg(long, default_value_t = 12)]
    max_zoom: u8,
    /// max number of threads
    #[arg(short, long, default_value_t = num_cpus::get())]
    threads: usize,
    /// output PNGs for debugging
    #[arg(long, default_value_t = false)]
    debug: bool,
}

fn save_buffer_as_png(data: &RasterBuffer, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut pixels = vec![0; (data.height() * data.width()) as usize];
    for i in 0..data.height() {
        for j in 0..data.width() {
            pixels[(i * data.width() + j) as usize] = data.get_pixel(i, j)? as u8;
        }
    }
    as_png(pixels, data.width() as u32, filename)
}

fn as_png(pixels: Vec<u8>, size: u32, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    let img = ImageBuffer::<Luma<u8>, Vec<u8>>::from_raw(size, size, pixels)
        .ok_or("Failed to create image buffer")?;
    img.save(filename)?;
    Ok(())
}

fn get_color(value: f64) -> u8 {
    for (i, t) in THRESHOLDS.iter().enumerate().rev() {
        if value >= *t {
            return COLORS[i];
        }
    }
    255
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();

    if args.debug {
        create_dir_all("debug")?;
    }

    let start = Instant::now();

    let processed_tiles = Arc::new(AtomicU64::new(0));

    {
        let processed_tiles = processed_tiles.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));

            loop {
                interval.tick().await;

                let count = processed_tiles.load(Ordering::Relaxed);

                let elapsed = start.elapsed().as_secs_f64();

                let rate = count as f64 / elapsed.max(0.001);

                println!(
                    "{:>12} tiles processed | {:>8.2} tiles/s | elapsed: {:>8.1}s",
                    count, rate, elapsed,
                );
            }
        });
    }

    let reader = Arc::new(ElevationReader::new(
        &args.input, TILE_SIZE, PADDING, args.min_zoom, args.max_zoom
    ).await?);

    let encoder = Arc::new(tokio::sync::Mutex::new(TileEncoder::new(
        &args.output,
        TILE_SIZE,
        PADDING,
        FEET_TO_METER,
    )?));

    let semaphore = Arc::new(Semaphore::new(args.threads));

    let tiles = reader.iter_tiles();

    tiles
        .map_err(|e| Box::<dyn std::error::Error>::from(e))
        .for_each_concurrent(args.threads, |tile_result| {
            let reader = reader.clone();
            let encoder = encoder.clone();
            let semaphore = semaphore.clone();
            let processed_tiles = processed_tiles.clone();

            async move {
                let permit = semaphore.acquire().await.unwrap();

                let tile = match tile_result {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("tile error: {e}");
                        return;
                    }
                };

                let mut elevation = RasterBuffer::zeros(
                    (TILE_SIZE + 2 * PADDING) as u64,
                    (TILE_SIZE + 2 * PADDING) as u64,
                    RasterDataType::Float64,
                );
                let mut bounds = ElevationBounds::default();

                for dx in -1i32..=1 {
                    for dy in -1i32..=1 {
                        if tile.x() as i32 + dx < 0 || tile.y() as i32 + dy < 0 {
                            continue;
                        }

                        let shifted_tile = match TileCoord::new(
                            tile.z(),
                            (tile.x() as i32 + dx) as u32,
                            (tile.y() as i32 + dy) as u32,
                        ) {
                            Ok(t) => t,
                            Err(_) => continue,
                        };

                        match reader.fill(&mut elevation, shifted_tile, dx, dy).await {
                            Err(e) => {
                                eprintln!("fill error: {e}");
                                return;
                            }
                            Ok(bnds) => bounds.merge(&bnds),
                        };
                    }
                }

                let results = tokio::task::spawn_blocking(move || {
                    let c =
                        ContourBuilder::new(TILE_SIZE + 2 * PADDING, TILE_SIZE + 2 * PADDING, true);

                    let contours_m = if args.contours_m && tile.z() >= 11 {
                        let thresholds =
                            bounds.get_thresholds(if tile.z() == 11 { 100.0 } else { 25.0 });
                        if thresholds.is_empty() {
                            Vec::new()
                        } else {
                            c.contours(elevation.as_slice().unwrap(), &thresholds)
                                .unwrap()
                        }
                    } else {
                        Vec::new()
                    };

                    let contours_ft = if args.contours_ft && tile.z() >= 11 {
                        let thresholds = bounds.get_thresholds(if tile.z() == 11 {
                            400.0 * FEET_TO_METER
                        } else {
                            100.0 * FEET_TO_METER
                        });
                        if thresholds.is_empty() {
                            Vec::new()
                        } else {
                            c.contours(elevation.as_slice().unwrap(), &thresholds)
                                .unwrap()
                        }
                    } else {
                        Vec::new()
                    };

                    let isobands = if args.hillshading {
                        let lat = ul(Tile::new(tile.x() as i32, tile.y() as i32, tile.z() as i32)).lat;
                        let pixel_size = 40075016.686 / (TILE_SIZE as f64) * lat.to_radians().cos() / 2f64.powf(tile.z() as f64);
                        let mut hillshade = swiss_hillshade(&elevation, 1.0, pixel_size).unwrap();

                        if args.debug {
                            save_buffer_as_png(&hillshade, &format!("debug/{}_{}_{}_hillshade.png", tile.z(), tile.x(), tile.y())).unwrap();
                        }

                        for i in 0..hillshade.height() {
                            for j in 0..hillshade.width() {
                                let val = hillshade.get_pixel(i, j).unwrap();
                                hillshade.set_pixel(i, j, 255.0 - val).unwrap();
                            }
                        }

                        if args.debug {
                            let mut classes = vec![0u8; (hillshade.height() * hillshade.width()) as usize];
                            for i in 0..hillshade.height() {
                                for j in 0..hillshade.width() {
                                    classes[(i * hillshade.width() + j) as usize] = get_color(hillshade.get_pixel(i as u64, j as u64).unwrap());
                                }
                            }
                            as_png(classes, hillshade.width() as u32, &format!("debug/{}_{}_{}_classes.png", tile.z(), tile.x(), tile.y())).unwrap();
                        }

                        c
                            .contours(hillshade.as_slice().unwrap(), &THRESHOLDS)
                            .unwrap()
                    } else {
                        Vec::new()
                    };

                    (contours_m, contours_ft, isobands)
                })
                .await
                .unwrap();

                {
                    let mut enc = encoder.lock().await;

                    if let Err(e) = enc.encode(tile, &results.0, &results.1, &results.2) {
                        eprintln!("encode error: {e}");
                    }
                }

                processed_tiles.fetch_add(1, Ordering::Relaxed);

                drop(permit);
            }
        })
        .await;

    Arc::try_unwrap(encoder).unwrap().into_inner().finalize()?;

    println!(
        "{:>12} tiles processed | {:>8.2} tiles/s | elapsed: {:>8.1}s",
        processed_tiles.load(Ordering::Relaxed),
        processed_tiles.load(Ordering::Relaxed) as f64 / start.elapsed().as_secs_f64(),
        start.elapsed().as_secs_f64(),
    );
    println!("done");

    Ok(())
}
