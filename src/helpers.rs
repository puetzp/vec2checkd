use handlebars::{
    Context as HandlebarsContext, Handlebars, Helper, Output, RenderContext, RenderError,
};

/// A handlebars helper that convert a floating point number to a string and truncates
/// it at a given number of decimals.
/// When the floating point number has no fractional part the default conversion to
/// a string is used (5.0_f64 => "5").
pub(crate) fn truncate(
    h: &Helper,
    _: &Handlebars,
    _: &HandlebarsContext,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> Result<(), RenderError> {
    let float = h
        .param(0)
        .ok_or_else(|| {
            RenderError::new(format!(
                "Helper \"{}\": missing a floating point number to truncate",
                h.name()
            ))
        })?
        .value()
        .as_f64()
        .ok_or_else(|| {
            RenderError::new(format!(
                "Helper \"{}\": failed to parse parameter as float",
                h.name()
            ))
        })?;

    match float.fract().classify() {
        std::num::FpCategory::Zero => out.write(&float.to_string())?,
        _ => match h.hash_get("prec") {
            None => out.write(&format!("{:.2}", float))?,
            Some(p) => {
                let precision = {
                    let i = p.value().as_u64().ok_or_else(|| {
                        RenderError::new(format!(
                            "Helper \"{}\": failed to parse parameter 'prec' as integer",
                            h.name()
                        ))
                    })?;
                    usize::try_from(i).unwrap()
                };

                out.write(&format!("{num:.prec$}", num = float, prec = precision))?
            }
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use handlebars::Handlebars;

    #[test]
    fn test_truncation_with_default() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("truncate", Box::new(truncate));
        let tpl = "{{truncate 5.5645654654654}}";
        assert_eq!(
            handlebars.render_template(tpl, &()).unwrap(),
            "5.56".to_string()
        );
    }

    #[test]
    fn test_truncation_with_default_no_fraction() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("truncate", Box::new(truncate));
        let tpl = "{{truncate 5.0}}";
        assert_eq!(
            handlebars.render_template(tpl, &()).unwrap(),
            "5".to_string()
        );
    }

    #[test]
    fn test_truncation_with_precision() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("truncate", Box::new(truncate));
        let tpl = "{{truncate prec=4 5.5645654654654}}";
        assert_eq!(
            handlebars.render_template(tpl, &()).unwrap(),
            "5.5646".to_string()
        );
    }

    #[test]
    fn test_truncation_with_precision_no_fraction() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("truncate", Box::new(truncate));
        let tpl = "{{truncate prec=4 5.0}}";
        assert_eq!(
            handlebars.render_template(tpl, &()).unwrap(),
            "5".to_string()
        );
    }

    #[test]
    fn test_truncation_wrong_precision_type() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("truncate", Box::new(truncate));
        let tpl = "{{truncate prec=foo 5.5645654654654}}";
        assert!(handlebars.render_template(tpl, &()).is_err());
    }

    #[test]
    fn test_truncation_no_arg() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("truncate", Box::new(truncate));
        let tpl = "{{truncate}}";
        assert!(handlebars.render_template(tpl, &()).is_err());
    }

    #[test]
    fn test_truncation_wrong_arg_type() {
        let mut handlebars = Handlebars::new();
        handlebars.register_helper("truncate", Box::new(truncate));
        let tpl = "{{truncate foo}}";
        assert!(handlebars.render_template(tpl, &()).is_err());
    }
}
