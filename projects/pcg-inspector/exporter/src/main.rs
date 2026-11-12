//! Consumer-owned JSON export of a RunenPCG graph candidate.
//! This is not RunenPCG's serialization or a framework-level wire format.
use runen_pcg::{
    ConnectedGraphAlgorithm, ConnectedGraphRequest, GenerationError, generate_connected_graph,
};
use std::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Options {
    nodes: u32,
    cycles: u32,
    max_edges: u32,
    seed: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            nodes: 8,
            cycles: 4,
            max_edges: 16,
            seed: 42,
        }
    }
}

fn parse_options(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(option) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {option}"))?;
        match option.as_str() {
            "--nodes" => {
                options.nodes = value
                    .parse()
                    .map_err(|_| "--nodes requires a u32".to_string())?;
            }
            "--cycles" => {
                options.cycles = value
                    .parse()
                    .map_err(|_| "--cycles requires a u32".to_string())?;
            }
            "--max-edges" => {
                options.max_edges = value
                    .parse()
                    .map_err(|_| "--max-edges requires a u32".to_string())?;
            }
            "--seed" => {
                options.seed = value
                    .parse()
                    .map_err(|_| "--seed requires a u64".to_string())?;
            }
            _ => return Err(format!("unrecognized option {option}")),
        }
    }
    Ok(options)
}

fn export(options: Options) -> Result<String, GenerationError> {
    let algorithm = ConnectedGraphAlgorithm::SpanningTreeV1;
    let graph = generate_connected_graph(ConnectedGraphRequest {
        node_count: options.nodes,
        minimum_cycle_rank: options.cycles,
        max_edges: options.max_edges,
        seed: options.seed,
        algorithm,
    })?;

    let mut json = format!(
        "{{\"format\":\"runen-lab.graph-view.v1\",\"algorithm\":\"{}\",\"seed\":\"{}\",\"node_count\":{},\"edges\":[",
        algorithm.id(),
        options.seed,
        graph.node_count()
    );
    for (index, edge) in graph.edges().iter().enumerate() {
        let (a, b) = edge.endpoints();
        if index != 0 {
            json.push(',');
        }
        json.push_str(&format!("[{a},{b}]"));
    }
    json.push_str("]}\n");
    Ok(json)
}

fn main() -> Result<(), Box<dyn Error>> {
    let options = parse_options(std::env::args().skip(1))?;
    print!("{}", export(options)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_generator_golden_fixture_is_reproduced() {
        let json = export(Options::default()).unwrap();
        assert_eq!(
            json,
            "{\"format\":\"runen-lab.graph-view.v1\",\"algorithm\":\"runen-pcg.graph.spanning-tree.v1\",\"seed\":\"42\",\"node_count\":8,\"edges\":[[0,1],[1,3],[1,5],[1,6],[1,7],[2,4],[2,6],[3,4],[4,5],[4,7],[6,7]]}\n"
        );
    }

    #[test]
    fn accepts_complete_u64_seed_without_losing_precision() {
        let options = parse_options(["--seed", "18446744073709551615"].map(str::to_owned))
            .expect("u64::MAX should be accepted");
        assert_eq!(options.seed, u64::MAX);
        assert!(export(options).unwrap().contains("\"seed\":\"18446744073709551615\""));
    }

    #[test]
    fn validates_cli_and_returns_generation_failure() {
        assert!(parse_options(["--nodes".into()]).is_err());
        assert!(parse_options(["--unknown".into(), "5".into()]).is_err());
        let options = Options {
            nodes: 0,
            ..Options::default()
        };
        assert_eq!(export(options), Err(GenerationError::InvalidNodeCount));
    }
}
