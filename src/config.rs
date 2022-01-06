use crate::types::Mapping;
use anyhow::{anyhow, Context};
use yaml_rust::yaml::Yaml;
use yaml_rust::YamlLoader;

fn parse_mapping(mapping: (&Yaml, &Yaml)) -> Result<Mapping, anyhow::Error> {
    let name = mapping
        .0
        .as_str()
        .ok_or(anyhow!("Failed to parse mappings.$name as string"))?
        .to_string();

    let items = mapping.1.as_hash().ok_or(anyhow!(
        "Failed to parse value of mappings.{} as hash",
        name
    ))?;

    let query = items
        .get(&Yaml::from_str("query"))
        .ok_or(anyhow!(
            "Failed to read mandatory attribute mappings.$name.query from configuration"
        ))?
        .as_str()
        .ok_or(anyhow!("Failed to parse mappings.{}.query as string", name))?
        .to_string();

    let host = items
        .get(&Yaml::from_str("host"))
        .ok_or(anyhow!(
            "Failed to read mandatory attribute mappings.$name.host from configuration"
        ))?
        .as_str()
        .ok_or(anyhow!("Failed to parse mappings.{}.host as string", name))?
        .to_string();

    let service = items
        .get(&Yaml::from_str("service"))
        .ok_or(anyhow!(
            "Failed to read mandatory attribute mappings.$name.service from configuration"
        ))?
        .as_str()
        .ok_or(anyhow!(
            "Failed to parse mappings.{}.service as string",
            name
        ))?
        .to_string();

    Ok(Mapping {
        name,
        query,
        host,
        service,
    })
}

pub(crate) fn parse_mappings_from_config(config: &str) -> Result<Vec<Mapping>, anyhow::Error> {
    let docs = YamlLoader::load_from_str(config)?;
    let doc = &docs[0];

    let mut mappings: Vec<Mapping> = vec![];

    let mapping_val = doc
        .as_hash()
        .ok_or(anyhow!("Failed to read configuration as hash"))?
        .get(&Yaml::from_str("mappings"))
        .ok_or(anyhow!(
            "Failed to read mandatory attribute 'mappings' from configuration"
        ))?;

    let mapping_hash = mapping_val.as_hash().ok_or(anyhow!(
        "Configuration attribute 'mappings' has the wrong type, expected a hash"
    ))?;

    for raw_mapping in mapping_hash {
        let parsed = parse_mapping(raw_mapping)?;
        mappings.push(parsed);
    }

    Ok(mappings)
}
