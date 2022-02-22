# Details on performance data

_Performance data_ can optionally be sent to Icinga as part of the passive check result if it is enabled in the _mapping_.
Performance data (as defined in [the Nagios dev. guidelines](https://nagios-plugins.org/doc/guidelines.html#AEN200) and [the Icinga2 doc.](https://icinga.com/docs/icinga-2/latest/doc/05-service-monitoring/#performance-data-metrics)) has the following general format:

```
'<label>'=<value><uom>;<warn>;<crit>;;

# Example:

'requests'=2000;@500:1000;@500;;
```

Note that vec2checkd ignores [min] and [max] and the label is single-quoted as it may contain whitespace. Warning and critical thresholds are also only inserted when they were in fact defined in the mapping, as is the unit-of-measurement.

## Default performance data

When performance data is enabled in the mapping configuration but no custom label is defined, vec2checkd computes performance data _labels_ on its own (the part before "=" in the example above, not to be mixed up with time series labels).

In general performance data labels must be uniquely identifiable **and** consistent across checks. In a fortunate happenstance this plays very well with he definition of _time series_, from the [Prometheus doc.](https://prometheus.io/docs/concepts/data_model/): "Prometheus fundamentally stores all data as time series: streams of timestamped values belonging to the same metric and the same set of labeled dimensions".

vec2checkd uses this trait to build default labels like so:

```
'<mapping name>/<md5_of_time_series_labels>'=...

e.g.

'Ingress requests/eaa8c4'=...
```

So vec2checkd takes computes the MD5 checksum from the set of labels for each time series, truncates it to the first 6 digits and prepends the mapping name to it. In general the first 6 digits should suffice to avoid collisions while keeping the mapping name in the label maintains readability.

Note that when a time series changes (and so its set of labels and metric name) a new performance data item is created because the MD5 checksum changed. The backend that ultimately handles the performance data will also start igoring the "old" performance data item and process the "new" one.

This can be avoided by customizing the performance data labels.

## Customizing performance data labels

Labels can be customized using a [handlebars template](https://handlebarsjs.com/) in the mapping configuration (see [the document that describes the YAML structure](configuration.md)). Vec2checkd then uses this template and a specific context to render performance data labels.
The _context_ in this case is a single object with all information that may be useful for rendering performance data labels. This context is evaluated per performance data and time series (in that it differs from the plugin output templating where the context is evaluated per check and thus contains way more information).
A context contains the following fields:

```
{
  name: "Nginx requests",
  host: "Kubernetes Production",
  service: "Nginx ingress requests per second",
  labels: {
    status: "404",
    exported_namespace: "my-app",
    ...
  }
}
```

The context above can then be used to define unique performance data labels. This is especially useful when you know that the time series in a PromQL result set differ only by a few or even a single time series label. Consider this mapping for example:

```yaml
mappings:
   ...
  'Nginx requests':
    query: 'sum by (status,exported_namespace) (rate(nginx_ingress_controller_requests{cluster="production"}[5m]))'
    host: 'Kubernetes Production'
    service: 'Nginx ingress reuqests per second'
    ...
    performance_data:
      enabled: true,
      label: '{{ labels.status }}/{{ labels.exported_namespace }}'
   ...
```

As we aggregate by the labels "status" and "exported_namespace" we know that the time series within the result set differ by those two labels. So the template that was used in the mapping will translate to an array of performance data like this:

```
'400/my-app'=...
'404/my-app'=...
'500/my-app'=...
'400/foo-app'=...
'404/foo-app'=...
'400/bar-app'=...
'404/bar-app'=...
'500/bar-app'=...
```

**Note:** vec2checkd will throw an error if it detects duplicate performance data labels and not send the passive check result. This is basically a safety measure as mixing up performance data labels might mess with your data further downstream.
Also note that as per the [specifics on templating](templating.md) processing the mapping will also fail if the expressions in the template cannot be evaluated because e.g. a field is missing ("strict mode"). For example generating proper performance data labels may fail for the mapping above when a single time series is contained in the result set that only has _one_ of the two labels referred to in the label template (either "status" _or_ "exported_namespace").
