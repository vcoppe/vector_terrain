use oxigdal_algorithms::raster::polygonize::PolygonFeature;
use mvt::{Error, GeomEncoder, GeomType, Tile};
use pmtiles::{PmTilesWriter, TileType, TileCoord};
use std::fs::File;

pub struct TileEncoder {}

impl TileEncoder {

    pub fn encode(tile_size: usize, tile_coord: TileCoord, polygons: &Vec<PolygonFeature>) -> Result<(), Error> {
        let mut tile = Tile::new(tile_size as u32);
        let mut layer = tile.create_layer("hillshading");

        for polygon_feature in polygons {
            let mut b = GeomEncoder::new(GeomType::Polygon);
            for coord in polygon_feature.polygon.exterior().coords() {
                b = b.point(coord.y, coord.x)?;
            }
            b.complete_geom()?;
            for interior in polygon_feature.polygon.interiors() {
                for coord in interior.coords() {
                    b = b.point(coord.y, coord.x)?;
                }
                b.complete_geom()?;
            }
            let data = b.encode()?;
            let mut feature = layer.into_feature(data);
            feature.add_tag_string("intensity", "todo");
            layer = feature.into_layer();
        }
        tile.add_layer(layer)?;
        let data = tile.to_bytes()?;

        let file = File::create("example.pmtiles").unwrap();
        let mut writer = PmTilesWriter::new(TileType::Mvt).create(file).unwrap();
        writer.add_tile(tile_coord, &data).unwrap();
        writer.finalize().unwrap();

        Ok(())
    }
}