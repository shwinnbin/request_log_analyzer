use std::io;
use std::io::prelude::*;
use std::net::TcpStream;
use std::env;
use std::process;

extern crate chrono;
use chrono::*;

extern crate aggregated_stats;

#[macro_use]
extern crate clap;

extern crate env_logger;
#[macro_use]
extern crate log;

extern crate prometheus;
extern crate hyper;
extern crate flate2;

extern crate failure;
use failure::Error;

mod analyzer;
mod args;
mod multi_file;
mod filter;
mod log_parser;
mod render;
mod request_response_matcher;
mod http_handler;
mod result;

fn main() {
    env_logger::init();

    let args = match args::parse_args(env::args()) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("Failed to parse arguments: {}", err);
            process::exit(1);
        }
    };

    if args.prometheus_listen.is_some() {
        let binding_address = args.prometheus_listen.clone().unwrap();
        http_handler::listen_http(args, &binding_address);
    } else {
        let result = run(&args);

        let mut stream;
        let mut stdout;

        let mut renderers: Vec<Box<render::Renderer>>;
        renderers = vec![];

        if !args.quiet {
            stdout = io::stdout();
            renderers.push(Box::new(
                render::terminal::TerminalRenderer::new(&mut stdout),
            ));
        }

        if args.graphite_server.is_some() {
            stream = match TcpStream::connect((
                args.graphite_server.as_ref().unwrap().as_str(),
                args.graphite_port.unwrap(),
            )) {
                Ok(stream) => stream,
                Err(err) => {
                    eprintln!("Could not connect to the Graphite server: {}", err);
                    process::exit(1);
                }
            };

            renderers.push(Box::new(render::graphite::GraphiteRenderer::new(
                Utc::now(),
                args.graphite_prefix.clone(),
                &mut stream,
            )));
        }

        if args.influxdb_write_url.is_some() {
            renderers.push(Box::new(render::influxdb::InfluxDbRenderer::new(
                &args.influxdb_write_url.clone().unwrap(),
                args.influxdb_tags.clone(),
            )));
        }

        for mut renderer in renderers {
            renderer.render(result.clone());
        }
    }
}

fn get_input(args: &args::RequestLogAnalyzerArgs) -> Result<Box<io::Read>, Error> {
    let input: Box<io::Read> = match args.filenames[0].as_ref() {
        "-" => Box::new(io::stdin()),
        _ => {
            let filenames = args.filenames.clone();
            Box::new(multi_file::MultiFile::new(filenames))
        }
    };
    Ok(input)
}

fn run(args: &args::RequestLogAnalyzerArgs) -> result::RequestLogAnalyzerResult {
    let input = match get_input(&args) {
        Ok(input) => input,
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    };

    let reader = io::BufReader::new(input);

    let mut events_iterator = reader
        .lines()
        .map(log_parser::parse_line)
        .filter(|event| event.is_ok())
        .map(|event| event.unwrap());

    let pairs_iterator = request_response_matcher::RequestResponsePairIterator::new(
        &mut events_iterator,
    ).filter(|pair| filter::matches_filter(pair, &args.conditions));

    analyzer::analyze_iterator(pairs_iterator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run() {
        let args = args::RequestLogAnalyzerArgs {
            filenames: vec![String::from("src/test/simple-1.log")],
            conditions: filter::FilterConditions {
                include_terms: None,
                exclude_terms: None,
                latest_time: None,
            },
            graphite_server: None,
            graphite_port: Some(2003),
            graphite_prefix: None,
            prometheus_listen: None,
            influxdb_write_url: None,
            influxdb_tags: None,
            quiet: false,
        };

        let result = run(&args);
        assert_eq!(result.count, 2);

        let timing = result.timing.unwrap();
        assert_eq!(timing.min, 7);
        assert_eq!(timing.max, 10);

        assert!(result.error.is_some());
    }

    #[test]
    fn test_get_input_file() {
        let args = args::RequestLogAnalyzerArgs {
            filenames: vec![String::from("src/test/simple-1.log")],
            conditions: filter::FilterConditions {
                include_terms: None,
                exclude_terms: None,
                latest_time: None,
            },
            graphite_server: None,
            graphite_port: Some(2003),
            graphite_prefix: None,
            prometheus_listen: None,
            influxdb_write_url: None,
            influxdb_tags: None,
            quiet: false,
        };

        let result = get_input(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_input_stdin() {
        let args = args::RequestLogAnalyzerArgs {
            filenames: vec![String::from("-")],
            conditions: filter::FilterConditions {
                include_terms: None,
                exclude_terms: None,
                latest_time: None,
            },
            graphite_server: None,
            graphite_port: Some(2003),
            graphite_prefix: None,
            prometheus_listen: None,
            influxdb_write_url: None,
            influxdb_tags: None,
            quiet: false,
        };

        let result = get_input(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_ignore_broken_lines() {
        let args = args::RequestLogAnalyzerArgs {
            filenames: vec![String::from("src/test/broken.log")],
            conditions: filter::FilterConditions {
                include_terms: None,
                exclude_terms: None,
                latest_time: None,
            },
            graphite_server: None,
            graphite_port: Some(2003),
            graphite_prefix: None,
            prometheus_listen: None,
            influxdb_write_url: None,
            influxdb_tags: None,
            quiet: false,
        };

        let result = run(&args);
        assert_eq!(result.count, 1);
    }

    #[test]
    fn test_run_multiple_files() {
        let args = args::RequestLogAnalyzerArgs {
            filenames: vec![
                String::from("src/test/simple-1.log"),
                String::from("src/test/simple-2.log"),
            ],
            conditions: filter::FilterConditions {
                include_terms: None,
                exclude_terms: None,
                latest_time: None,
            },
            graphite_server: None,
            graphite_port: Some(2003),
            graphite_prefix: None,
            prometheus_listen: None,
            influxdb_write_url: None,
            influxdb_tags: None,
            quiet: false,
        };

        let result = run(&args);
        assert_eq!(result.count, 4);
    }

    #[test]
    fn test_run_gzipped_file() {
        let args = args::RequestLogAnalyzerArgs {
            filenames: vec![String::from("src/test/simple-1.log.gz")],
            conditions: filter::FilterConditions {
                include_terms: None,
                exclude_terms: None,
                latest_time: None,
            },
            graphite_server: None,
            graphite_port: Some(2003),
            graphite_prefix: None,
            prometheus_listen: None,
            influxdb_write_url: None,
            influxdb_tags: None,
            quiet: false,
        };

        let result = run(&args);
        assert_eq!(result.count, 2);
    }
}
