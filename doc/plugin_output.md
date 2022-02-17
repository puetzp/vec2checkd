# Details on the plugin output

## Default output

Specifying a value for the plugin output in a mapping is optional. When no value is provided different default (single-line) outputs are sent to the Icinga API, depending on the overall exit status of the check, whether the passive check result updates an Icinga host or service object and the number of result vectors returned by the PromQL query.


**Defaults for Icinga services**

Case #1: PromQL query returned a single item.

exit value | plugin output | example
--- | --- | ---
0 | [OK] PromQL query returned one result (\<value\>) | [OK] PromQL query returned one result (5.56)
1 | [WARNING] PromQL query returned one result within the warning range (\<value\> in \<range\>) | [WARNING] PromQL query returned one result within the warning range (12.15 in @10:20)
2 | [CRITICAL] PromQL query returned one result within the critical range  (\<value\> in \<range\>) | [CRITICAL] PromQL query returned one result within the critical range (22.86 in @0:30)

Case #2: PromQL query returned multiple items.

exit value | plugin output | example
--- | --- | ---
0 | [OK] PromQL query returned multiple results in the range \<min\>..=\<max\> | [OK] PromQL query returned multiple results in the range 0.05..=6.74
1 | [WARNING] PromQL query returned multiple results within the warning range (values \<min\>..=\<max\> overlap with \<range\> | [WARNING] PromQL query returned multiple results within the warning range (values 12..=20 overlap with @10:20)
1 | [CRITICAL] PromQL query returned multiple results within the critical range (values \<min\>..=\<max\> overlap with \<range\> | [CRITICAL] PromQL query returned multiple results within the critical range (values 75.50..=110 overlap with @0:100)

Case #3: PromQL query returned no result at all

exit value | plugin output
--- | ---
3 | [UNKNOWN] PromQL query result set is empty


**Defaults for Icinga hosts**

The output for host objects will look almost the same. Only the service/host status in the square brackets in the tables above will be mapped accordingly:

* exit value 0 -> [UP]
* exit value 1 -> [UP]
* exit value 2 -> [DOWN]

## Customized output

The default output above is only useful to display the results of trivial PromQL queries where e.g. the name of the Icinga service object gives enough context to explain and complement the presented values. The default is also generic because its difficult to assume which time series labels are relevant and may be useful to include in the output.

For that reason it is also possible to provide a [handlebars template](https://handlebarsjs.com/) in the configuration (see [the document that describes the YAML structure](configuration.md). vec2checkd then uses this template and a specific context to render the plugin output.
The _context_ in this case is a single object that contains all the information that accumulated from the evaluation of a PromQL query that you may want to include in the output:

```json
{
  # the name of the mapping
  name: "Node status",
  # the PromQL query defined in the mapping
  query: "kube_node_status_condition{cluster="test",condition!="Ready",status="true"}",
  # the thresholds from the mapping
  thresholds: {
    warning: "@0:100",
    critical: "@101:"
  },
  # Icinga host object
  host: "Kubernetes Production",
  # Icinga service object
  service: "Node status",
  # check interval
  interval: 60,
  # overall "plugin" exit code
  exit_value: 0,
  # overall "plugin" exit status
  exit_status: "OK",
  # some helpers regarding the overall "plugin" status
  is_ok: true,
  is_warning: false,
  is_critical: false,
  # All data points, i.e. time series returned by the query, further enriched with some evaluation data
  data: [
    {
      # Labels in this time series
      labels: {
        __name__: "kube_node_status_condition",
	job: "kubernetes-service-endpoints",
	instance: "...:8080",
	namespace: "default",
	exported_node: "worker1",
	...
      },
      # Float value from query result
      value: 0.0,
      # The per-time-series results from checking the value against the thresholds
      exit_value: 0,
      exit_status: "OK",
      is_ok: true,
      is_warning: false,
      is_critical: false
    },
    {
    ...
    },
    ...
  ]
}
```




