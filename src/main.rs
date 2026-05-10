mod elevation_reader;
mod tile_encoder;

use clap::{ArgAction, Parser};
use contour::ContourBuilder;
use futures::{StreamExt, TryStreamExt};
use oxigdal_algorithms::raster::swiss_hillshade;
use oxigdal_core::buffer::RasterBuffer;
use oxigdal_core::types::RasterDataType;
use pmtiles::TileCoord;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

use elevation_reader::ElevationReader;
use tile_encoder::TileEncoder;

use crate::elevation_reader::ElevationBounds;

const TILE_SIZE: usize = 512;
const PADDING: usize = 16;
const THRESHOLDS: [f64; 4] = [96.0, 112.0, 140.0, 256.0];
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
    #[arg(
        long,
        action = ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = false,
    )]
    hillshading: bool,
    /// compute contour lines (meters)
    #[arg(
        long,
        action = ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = false,
    )]
    lines_m: bool,
    /// compute contour lines (feet)
    #[arg(
        long,
        action = ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = false,
    )]
    lines_ft: bool,
    /// ignore tiles below minimum zoom
    #[arg(long, default_value_t = 4)]
    min_zoom: u8,
    /// ignore tiles above maximum zoom
    #[arg(long, default_value_t = 12)]
    max_zoom: u8,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();

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

    let concurrency = num_cpus::get();

    let semaphore = Arc::new(Semaphore::new(concurrency));

    let tiles = reader.iter_tiles();

    tiles
        .map_err(|e| Box::<dyn std::error::Error>::from(e))
        .for_each_concurrent(concurrency, |tile_result| {
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
                    let mut hillshade = swiss_hillshade(&elevation, 1.0, 30.0).unwrap();

                    for i in 0..hillshade.height() {
                        for j in 0..hillshade.width() {
                            let val = hillshade.get_pixel(i, j).unwrap();

                            hillshade.set_pixel(i, j, 255.0 - val).unwrap();
                        }
                    }

                    let c =
                        ContourBuilder::new(TILE_SIZE + 2 * PADDING, TILE_SIZE + 2 * PADDING, true);

                    let contours_m = if args.lines_m && tile.z() >= 11 {
                        let thresholds =
                            bounds.get_thresholds(if tile.z() == 11 { 50.0 } else { 10.0 });
                        if thresholds.is_empty() {
                            Vec::new()
                        } else {
                            c.contours(elevation.as_slice().unwrap(), &thresholds)
                                .unwrap()
                        }
                    } else {
                        Vec::new()
                    };

                    let contours_ft = if args.lines_ft && tile.z() >= 11 {
                        let thresholds = bounds.get_thresholds(if tile.z() == 11 {
                            200.0 * FEET_TO_METER
                        } else {
                            40.0 * FEET_TO_METER
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
                        c
                            .isobands(hillshade.as_slice().unwrap(), &THRESHOLDS)
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
