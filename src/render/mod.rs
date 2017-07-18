pub mod prometheus;

use std::io::prelude::*;
use chrono::*;
use result;
use timing_analyzer;

pub trait Renderer {
    fn render(&mut self, result: result::RequestLogAnalyzerResult) -> ();
}

pub struct TerminalRenderer {}

impl TerminalRenderer {
    pub fn new() -> TerminalRenderer {
        TerminalRenderer {}
    }
}

impl Renderer for TerminalRenderer {
    fn render(&mut self, result: result::RequestLogAnalyzerResult) -> () {
        println!("count:\t{}", result.count);
        match result.timing {
            Some(timing) => {
                println!("time.avg:\t{}", timing.avg);
                println!("time.min:\t{}", timing.min);
                println!("time.median:\t{}", timing.median);
                println!("time.90percent:\t{}", timing.percentile90);
                println!("time.max:\t{}", timing.max);
            }
            None => warn!("No matching log lines in file."),
        }
    }
}

pub struct GraphiteRenderer<'a> {
    time: DateTime<UTC>,
    prefix: Option<String>,
    stream: &'a mut Write,
}

impl<'a> GraphiteRenderer<'a> {
    pub fn new(time: DateTime<UTC>,
               prefix: Option<String>,
               stream: &'a mut Write)
               -> GraphiteRenderer<'a> {
        GraphiteRenderer {
            time: time,
            prefix: prefix,
            stream: stream,
        }
    }
}

impl<'a> Renderer for GraphiteRenderer<'a> {
    fn render(&mut self, result: result::RequestLogAnalyzerResult) -> () {
        let prefix_text: String;
        let prefix_separator: &str;

        match self.prefix {
            Some(ref p) => {
                prefix_text = p.clone();
                prefix_separator = ".";
            }
            None => {
                prefix_text = String::from("");
                prefix_separator = "";
            }
        };

        let mut write = |text: String| {
            let _ = self.stream.write(format!("{}{}{} {}\n",
                                              prefix_text,
                                              prefix_separator,
                                              text,
                                              self.time.timestamp())
                .as_bytes());
        };

        write(format!("requests.count {}", result.count));

        match result.timing {
            Some(timing) => {
                write(format!("requests.time.max {}", timing.max));
                write(format!("requests.time.min {}", timing.min));
                write(format!("requests.time.avg {}", timing.avg));
                write(format!("requests.time.median {}", timing.median));
                write(format!("requests.time.90percent {}", timing.percentile90));
            }
            None => warn!("No matching log lines in file."),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::io::prelude::*;
    use std::str;
    use chrono::*;
    use timing_analyzer;
    use super::*;

    struct MockTcpStream {
        write_calls: Vec<String>,
    }

    impl Write for MockTcpStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.write_calls.push(str::from_utf8(buf).unwrap().to_string());
            Ok(1)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn get_result_fixture() -> result::RequestLogAnalyzerResult {
        result::RequestLogAnalyzerResult {
            count: 3,
            timing: Some(timing_analyzer::RequestLogAnalyzerResult {
                max: 100,
                min: 1,
                avg: 37,
                median: 10,
                percentile90: 100,
            }),
            error: None,
        }
    }

    fn get_time_fixture() -> DateTime<UTC> {
        let time: DateTime<UTC> = DateTime::parse_from_str("22/Sep/2016:22:41:59 +0200",
                                                           "%d/%b/%Y:%H:%M:%S %z")
            .unwrap()
            .with_timezone(&UTC);

        time
    }

    #[test]
    fn test_terminal_renderer() {
        let mut renderer = TerminalRenderer::new();

        let result = get_result_fixture();

        renderer.render(result);
    }

    #[test]
    fn test_render_graphite() {
        let mut mock_tcp_stream = MockTcpStream { write_calls: vec![] };

        {
            let mut renderer =
                GraphiteRenderer::new(get_time_fixture(), None, &mut mock_tcp_stream);
            renderer.render(get_result_fixture());
        }

        assert_eq!(&mock_tcp_stream.write_calls[0],
                   "requests.count 3 1474576919\n");
        assert_eq!(&mock_tcp_stream.write_calls[1],
                   "requests.time.max 100 1474576919\n");
        assert_eq!(&mock_tcp_stream.write_calls[2],
                   "requests.time.min 1 1474576919\n");
        assert_eq!(&mock_tcp_stream.write_calls[3],
                   "requests.time.avg 37 1474576919\n");
        assert_eq!(&mock_tcp_stream.write_calls[4],
                   "requests.time.median 10 1474576919\n");
        assert_eq!(&mock_tcp_stream.write_calls[5],
                   "requests.time.90percent 100 1474576919\n");
    }

    #[test]
    fn test_render_graphite_with_prefix() {
        let mut mock_tcp_stream = MockTcpStream { write_calls: vec![] };

        {
            let mut renderer = GraphiteRenderer::new(get_time_fixture(),
                                                     Some(String::from("my_prefix")),
                                                     &mut mock_tcp_stream);
            renderer.render(get_result_fixture());
        }

        assert_eq!(&mock_tcp_stream.write_calls[0],
                   "my_prefix.requests.count 3 1474576919\n");
        assert_eq!(&mock_tcp_stream.write_calls[1],
                   "my_prefix.requests.time.max 100 1474576919\n");
        assert_eq!(&mock_tcp_stream.write_calls[2],
                   "my_prefix.requests.time.min 1 1474576919\n");
        assert_eq!(&mock_tcp_stream.write_calls[3],
                   "my_prefix.requests.time.avg 37 1474576919\n");
        assert_eq!(&mock_tcp_stream.write_calls[4],
                   "my_prefix.requests.time.median 10 1474576919\n");
        assert_eq!(&mock_tcp_stream.write_calls[5],
                   "my_prefix.requests.time.90percent 100 1474576919\n");
    }
}
