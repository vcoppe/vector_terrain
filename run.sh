#!/bin/bash

cargo build --release

./target/release/vector_terrain --input planet.pmtiles --output hillshading.pmtiles --hillshading
./target/release/vector_terrain --input planet.pmtiles --output contours_m.pmtiles --contours-m
./target/release/vector_terrain --input planet.pmtiles --output contours_ft.pmtiles --contours-ft