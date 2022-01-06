use anyhow::anyhow;
use std::iter::Peekable;

pub(crate) enum NagiosRange {
    Inside(Start, End),
    Outside(Start, End),
}

pub(crate) enum Start {
    Value(usize),
    NegInf,
}

pub(crate) enum End {
    Value(usize),
    PosInf,
}

fn parse_range<I: Iterator>(range: &mut Peekable<I>) -> (Start, End) {
    (Start::NegInf, End::PosInf)
}

impl std::str::FromStr for NagiosRange {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut chars = input.chars().peekable();

        match chars.peek() {
            Some(c) => match c {
                '@' => {
                    chars.next();
                    let (start, end) = parse_range(&mut chars);
                    let inside_range = NagiosRange::Inside(start, end);
                    Ok(inside_range)
                }
                _ => {
                    let (start, end) = parse_range(&mut chars);
                    let outside_range = NagiosRange::Outside(start, end);
                    Ok(outside_range)
                }
            },
            None => return Err(anyhow!("Nagios range expression is empty")),
        }
    }
}

pub(crate) struct ThresholdPair {
    pub warning: NagiosRange,
    pub critical: NagiosRange,
}

pub(crate) struct Mapping {
    pub name: String,
    pub query: String,
    //    pub thresholds: ThresholdPair,
    pub host: String,
    pub service: String,
}
