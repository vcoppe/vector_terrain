use contour::{Band, Contour};
use geo_types::LineString;
use mvt::{Error as MvtError, GeomEncoder, GeomType, Tile};
use pmtiles::{PmTilesStreamWriter, PmTilesWriter, PmtError, TileCoord, TileType};
use std::fmt::Debug;
use std::fs::File;

const EXPANSION_FACTOR: usize = 8; // to avoid snapping coordinates to the 512x512 grid
const EXPANSION_FACTOR_FLOAT: f64 = EXPANSION_FACTOR as f64;

#[derive(Debug)]
pub enum TileEncoderError {
    Mvt(MvtError),
    PmTiles(PmtError),
    Io(std::io::Error),
}

impl From<MvtError> for TileEncoderError {
    fn from(err: MvtError) -> Self {
        TileEncoderError::Mvt(err)
    }
}

impl From<PmtError> for TileEncoderError {
    fn from(err: PmtError) -> Self {
        TileEncoderError::PmTiles(err)
    }
}

impl From<std::io::Error> for TileEncoderError {
    fn from(err: std::io::Error) -> Self {
        TileEncoderError::Io(err)
    }
}

impl std::fmt::Display for TileEncoderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TileEncoderError::Mvt(e) => write!(f, "MVT error: {}", e),
            TileEncoderError::PmTiles(e) => write!(f, "PmTiles error: {}", e),
            TileEncoderError::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for TileEncoderError {}

pub struct TileEncoder {
    writer: PmTilesStreamWriter<File>,
    tile_size: usize,
    padding: usize,
    feet_to_meter: f64,
}

impl Debug for TileEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TileEncoder")
            .field("tile_size", &self.tile_size)
            .field("padding", &self.padding)
            .finish()
    }
}

impl TileEncoder {
    pub fn new(
        path: &str,
        tile_size: usize,
        padding: usize,
        feet_to_meter: f64,
    ) -> Result<Self, TileEncoderError> {
        let file = File::create(path)?;
        let writer = PmTilesWriter::new(TileType::Mvt).create(file)?;
        Ok(Self {
            writer,
            tile_size,
            padding,
            feet_to_meter,
        })
    }

    pub fn encode(
        &mut self,
        tile_coord: TileCoord,
        contours_m: &Vec<Contour>,
        contours_ft: &Vec<Contour>,
        bands: &Vec<Band>,
    ) -> Result<(), TileEncoderError> {
        let mut tile = Tile::new((self.tile_size * EXPANSION_FACTOR) as u32);

        self.encode_contours(&mut tile, contours_m, true)?;
        self.encode_contours(&mut tile, contours_ft, false)?;
        self.encode_hillshading(&mut tile, bands)?;

        let data = tile.to_bytes()?;

        self.writer.add_tile(tile_coord, &data)?;

        Ok(())
    }

    pub fn finalize(self) -> Result<(), TileEncoderError> {
        self.writer.finalize()?;

        Ok(())
    }

    fn encode_contours(
        &self,
        tile: &mut Tile,
        contours: &Vec<Contour>,
        metric: bool,
    ) -> Result<(), TileEncoderError> {
        let mut layer = tile.create_layer(if metric { "contours_m" } else { "contours_ft" });

        for contour in contours.iter() {
            let ele = if metric {
                contour.threshold()
            } else {
                (contour.threshold() / self.feet_to_meter).round()
            } as i64;
            for polygon in contour.geometry().iter() {
                {
                    let mut b = GeomEncoder::new(GeomType::Linestring);
                    b = self.add_linestring(b, polygon.exterior())?;
                    let data = b.encode()?;
                    let mut feature = layer.into_feature(data);
                    feature.add_tag_int("ele", ele);
                    layer = feature.into_layer();
                }
                for interior in polygon.interiors() {
                    let mut b = GeomEncoder::new(GeomType::Linestring);
                    b = self.add_linestring(b, interior)?;
                    let data = b.encode()?;
                    let mut feature = layer.into_feature(data);
                    feature.add_tag_int("ele", ele);
                    layer = feature.into_layer();
                }
            }
        }
        tile.add_layer(layer)?;

        Ok(())
    }

    fn encode_hillshading(
        &self,
        tile: &mut Tile,
        bands: &Vec<Band>,
    ) -> Result<(), TileEncoderError> {
        let mut layer = tile.create_layer("hillshading");
        for band in bands.iter() {
            for polygon in band.geometry().iter() {
                let mut b = GeomEncoder::new(GeomType::Polygon);
                b = self.add_linestring(b, polygon.exterior())?;
                for interior in polygon.interiors() {
                    b = self.add_linestring(b, interior)?;
                }
                let data = b.encode()?;
                let feature = layer.into_feature(data);
                layer = feature.into_layer();
            }
        }
        tile.add_layer(layer)?;

        Ok(())
    }

    fn add_linestring(
        &self,
        mut encoder: GeomEncoder<f64>,
        line: &LineString<f64>,
    ) -> Result<GeomEncoder<f64>, TileEncoderError> {
        for coord in line.coords() {
            encoder = encoder.point(
                (coord.y - self.padding as f64) * EXPANSION_FACTOR_FLOAT,
                (coord.x - self.padding as f64) * EXPANSION_FACTOR_FLOAT,
            )?;
        }
        encoder.complete_geom()?;
        Ok(encoder)
    }
}
