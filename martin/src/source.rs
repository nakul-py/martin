use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;

use actix_web::error::ErrorNotFound;
use async_trait::async_trait;
use itertools::Itertools;
use log::debug;
use martin_tile_utils::TileInfo;
use serde::{Deserialize, Serialize};
use tilejson::TileJSON;

use crate::{Result, Xyz};

pub type Tile = Vec<u8>;
pub type UrlQuery = HashMap<String, String>;

pub type TileInfoSource = Box<dyn Source>;

pub type TileInfoSources = Vec<TileInfoSource>;

#[derive(Default, Clone)]
pub struct TileSources(HashMap<String, Box<dyn Source>>);
pub type TileCatalog = BTreeMap<String, CatalogSourceEntry>;

impl TileSources {
    #[must_use]
    pub fn new(sources: Vec<TileInfoSources>) -> Self {
        Self(
            sources
                .into_iter()
                .flatten()
                .map(|src| (src.get_id().to_string(), src))
                .collect(),
        )
    }

    pub fn get_catalog(&self) -> TileCatalog {
        self.0
            .iter()
            .map(|(id, src)| (id.to_string(), src.get_catalog_entry()))
            .sorted_by(|(id1, _), (id2, _)| id1.cmp(id2))
            .collect()
    }

    pub fn get_source(&self, id: &str) -> actix_web::Result<&dyn Source> {
        Ok(self
            .0
            .get(id)
            .ok_or_else(|| ErrorNotFound(format!("Source {id} does not exist")))?
            .as_ref())
    }

    pub fn get_sources(
        &self,
        source_ids: &str,
        zoom: Option<u8>,
    ) -> actix_web::Result<(Vec<&dyn Source>, bool, TileInfo)> {
        let mut sources = Vec::new();
        let mut info: Option<TileInfo> = None;
        let mut use_url_query = false;
        for id in source_ids.split(',') {
            let src = self.get_source(id)?;
            let src_inf = src.get_tile_info();
            use_url_query |= src.support_url_query();

            // make sure all sources have the same format
            match info {
                Some(inf) if inf == src_inf => {}
                Some(inf) => Err(ErrorNotFound(format!(
                    "Cannot merge sources with {inf} with {src_inf}"
                )))?,
                None => info = Some(src_inf),
            }

            // TODO: Use chained-if-let once available
            if match zoom {
                Some(zoom) if Self::check_zoom(src, id, zoom) => true,
                None => true,
                _ => false,
            } {
                sources.push(src);
            }
        }

        // format is guaranteed to be Some() here
        Ok((sources, use_url_query, info.unwrap()))
    }

    pub fn check_zoom(src: &dyn Source, id: &str, zoom: u8) -> bool {
        let is_valid = src.is_valid_zoom(zoom);
        if !is_valid {
            debug!("Zoom {zoom} is not valid for source {id}");
        }
        is_valid
    }
}

#[async_trait]
pub trait Source: Send + Debug {
    fn get_id(&self) -> &str;

    fn get_tilejson(&self) -> &TileJSON;

    fn get_tile_info(&self) -> TileInfo;

    fn clone_source(&self) -> Box<dyn Source>;

    fn support_url_query(&self) -> bool;

    async fn get_tile(&self, xyz: &Xyz, query: &Option<UrlQuery>) -> Result<Tile>;

    fn is_valid_zoom(&self, zoom: u8) -> bool {
        let tj = self.get_tilejson();
        tj.minzoom.map_or(true, |minzoom| zoom >= minzoom)
            && tj.maxzoom.map_or(true, |maxzoom| zoom <= maxzoom)
    }

    fn get_catalog_entry(&self) -> CatalogSourceEntry {
        let id = self.get_id();
        let tilejson = self.get_tilejson();
        let info = self.get_tile_info();
        CatalogSourceEntry {
            content_type: info.format.content_type().to_string(),
            content_encoding: info.encoding.content_encoding().map(ToString::to_string),
            name: tilejson.name.as_ref().filter(|v| *v != id).cloned(),
            description: tilejson.description.clone(),
            attribution: tilejson.attribution.clone(),
        }
    }
}

impl Clone for Box<dyn Source> {
    fn clone(&self) -> Self {
        self.clone_source()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogSourceEntry {
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_encoding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xyz_format() {
        let xyz = Xyz { z: 1, x: 2, y: 3 };
        assert_eq!(format!("{xyz}"), "1,2,3");
        assert_eq!(format!("{xyz:#}"), "1/2/3");
    }
}
