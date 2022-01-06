use anyhow::anyhow;
use std::iter::Peekable;

pub(crate) enum NagiosRange {
    Inside(Start, End),
    Outside(Start, End),
}

pub(crate) enum Start {
    Value(i32),
    NegInf,
}

pub(crate) enum End {
    Value(i32),
    PosInf,
}

fn parse_range<I: Iterator<Item = char>>(
    range: &mut Peekable<I>,
) -> Result<(Start, End), anyhow::Error> {
    let next = range
        .next()
        .ok_or(anyhow!("Nagios range expression is invalid"))?;

    let (start, end) = match next {
        '~' => {
            let start = Start::NegInf;

            let end = match range
                .next()
                .ok_or(anyhow!("Nagios range expression is invalid"))?
            {
                ':' => {
                    let num = range.collect::<String>().parse::<i32>()?;
                    End::Value(num);
                }
                _ => return Err(anyhow!("Nagios range Expression is invalid")),
            };

            (start, End::PosInf)
        }
        _ => {
            let num = range
                .take_while(|c| c.is_numeric())
                .collect::<String>()
                .parse::<i32>()?;

            match range.next() {
                Some(c) => match c {
                    ':' => {
                        let start = Start::Value(num);
                        let num = range.collect::<String>().parse::<i32>()?;
                        let end = End::Value(num);
                        (start, end)
                    }
                    _ => return Err(anyhow!("Nagios range expression is invalid")),
                },
                None => (Start::Value(0), End::Value(num)),
            }
        }
    };
    Ok((start, end))
}

impl std::str::FromStr for NagiosRange {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut chars = input.chars().peekable();

        match chars.peek() {
            Some(c) => match c {
                '@' => {
                    chars.next();
                    let (start, end) = parse_range(&mut chars)?;
                    let inside_range = NagiosRange::Inside(start, end);
                    Ok(inside_range)
                }
                _ => {
                    let (start, end) = parse_range(&mut chars)?;
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
