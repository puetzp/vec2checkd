# Templating with handlebars

vec2checkd uses [handlebars templates](https://handlebarsjs.com/) optionally in two instances, i.e. customizing the plugin output and performance data labels of a check/mapping.

Handlebars uses a _context_ to substitute expressions in a _template_ and produce a string. This context is pre-defined by vec2checkd in both instances (plugin output/performance data). Please refer to the documentation on [plugin output](plugin_output.md) and [performance data](performance_data.md) for specifics on the context and the variables that can thus be used in a custom template.

This document contains just some general information regarding the [handlebars implementation](https://github.com/sunng87/handlebars-rust) and how is used by vec2checkd.

* this version of [handlebars](https://github.com/sunng87/handlebars-rust) implements only essential helpers listed [here](https://docs.rs/handlebars/4.2.1/handlebars/#built-in-helpers)
* vec2checkd implements one custom helper that may be used in templates called "truncate". "truncate" can be used to reduce the precision of a float value in places where the exact number does not matter (e.g. plugin output) to a specific number of decimals.
  - Call with optional precision: "{{ truncate prec=4 <float> }}"
  - Call without precision (default 2): "{{ truncate <float> }}"
* vec2checkd uses handlebars in [strict mode](https://docs.rs/handlebars/4.2.1/handlebars/#strict-mode). So in general rendering a template that access a non-existing field that is not part of the _context_ will fail. However in certain cases accessing a non-existing field will not fail, e.g. when this field is a parameter to a built-in helper like "#if". Keeping this in mind will probably save you some time when you cannot fathom why the plugin output in Icinga does not look as you expected.
