# Details on the plugin output

## Default output

Specifying a value for the plugin output in a mapping is optional. When no value is provided different default (single-line) outputs are sent to the Icinga API, depending on the overall exit status of the check, whether the passive check result updates an Icinga host or service object and the number of result vectors returned by the PromQL query.


**Defaults for Icinga services**

Case #1: PromQL query returned a single item.

evaluation | example
--- | ---
no threshold was hit / no thresholds defined | [OK] PromQL query returned one result (5.56)
warning threshold was hit | [WARNING] PromQL query returned one result within the warning range (12.15 in @10:20)
critical threshold was hit| [CRITICAL] PromQL query returned one result within the critical range (22.86 in @0:30)
no result from PromQL query | [UNKNOWN] PromQL query result set is empty

Case #2: PromQL query returned multiple items.

evaluation | example
--- | ---
no threshold was hit | [OK] PromQL query returned multiple results in the range 0.05..=6.74
warning threshold was hit | [WARNING] PromQL query returned multiple results within the warning range (values 12..=20 overlap with @10:20)
critical threshold was hit | [CRITICAL] PromQL query returned multiple results within the critical range (values 75.50..=110 overlap with @0:100)
no result from PromQL query | [UNKNOWN] PromQL query result set is empty


**Defaults for Icinga hosts**

Case #1: PromQL query returned a single item.

evaluation | example
--- | ---
no threshold was hit / no thresholds defined | [UP] PromQL query returned one result (5.56)
warning threshold was hit | [UP] PromQL query returned one result within the warning range (12.15 in @10:20)
critical threshold was hit| [DOWN] PromQL query returned one result within the critical range (22.86 in @0:30)
no result from PromQL query | [DOWN] PromQL query result set is empty

Case #2: PromQL query returned multiple items.

evaluation | example
--- | ---
no threshold was hit | [UP] PromQL query returned multiple results in the range 0.05..=6.74
warning threshold was hit | [UP] PromQL query returned multiple results within the warning range (values 12..=20 overlap with @10:20)
critical threshold was hit | [DOWN] PromQL query returned multiple results within the critical range (values 75.50..=110 overlap with @0:100)
no result from PromQL query | [DOWN] PromQL query result set is empty

## Customized output

The default output above is only useful to display the results of trivial PromQL queries where e.g. the name of the Icinga service object gives enough context to explain and complement the presented values. The default is also generic because its difficult to assume which time series labels are relevant and may be useful to include in the output.

For that reason it is also possible to provide a [handlebars template](https://handlebarsjs.com/) in the configuration (see [the document that describes the YAML structure](configuration.md). vec2checkd then uses this template and a specific context to render the plugin output.
The _context_ in this case is a single object that contains all the information that accumulated from the evaluation of a PromQL query that you may want to include in the output:

```
{
  name: "Node status",
  query: "kube_node_status_condition{cluster="test",condition!="Ready",status="true"}",
  thresholds: {
    warning: "@0:100",
    critical: "@101:"
  },
  host: "Kubernetes Production",
  service: "Node status",
  interval: 60,

  # overall "plugin" status and helper variables
  exit_value: 0,
  exit_status: "OK",
  is_ok: true,
  is_warning: false,
  is_critical: false,

  # All data points, i.e. time series returned by the query, further enriched with some evaluation data
  data: [
    {
      labels: {
        __name__: "kube_node_status_condition",
	job: "kubernetes-service-endpoints",
	instance: "...:8080",
	namespace: "default",
	exported_node: "worker1",
	...
      },
      value: 0.0,

      # per-time-series results from checking the value against the thresholds
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

**Note:** The context differs slightly when it is used to render output for Icinga host objects. In this case it does not contain **is_ok**, **is_warning** and **is_critical** but **is_up** and **is_down**

Using the context above translates the following mapping:

```
mappings:
  # ...
  Node status:
    query: kube_node_status_condition{cluster="test",condition="Ready",status="true"}
    host: Kubernetes (Test-Cluster)
    interval: 30
    thresholds:
      critical: "1:"
    plugin_output: |
      {{ #if is_up }}
      [{{ exit_status }}] All nodes are ready and report no problems
      {{ else }}
      [{{ exit_status }}] At least one node has a problem
      {{ /if }}
      {{ #each data }}
      {{ #if this.is_down }}
      [{{ this.exit_status }}] Kubelet on {{ this.labels.exported_node }} is not ready
      {{ /if }}
      {{ /each }}
    performance_data:
      enabled: false
```

... to the output:

```
[UP] All nodes are ready and report no problems
```

... and if a node transitions to a "not ready" state the same template produces:

```
[DOWN] At least one node has a problem

[DOWN] Kubelet on worker1 is not ready
```
