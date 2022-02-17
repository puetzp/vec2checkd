# Details on the plugin output

Specifying a value for the plugin output in a mapping is optional. When no value is provided different default outputs are sent to the Icinga API, depending on the overall exit status of the check, whether the passive check result updates an Icinga host or service object and the number of result vectors returned by the PromQL query.

Case #1: Update a service, PromQL query returned a single item.

exit status | plugin output | example
--- | --- | ---
0 | [OK] PromQL query returned one result (\<value\>) | [OK] PromQL query returned one result (5.56)
1 | [WARNING] PromQL query returned one result within the warning range (\<value\> in \<range\>) | [WARNING] PromQL query returned one result within the warning range (12.15 in @10:20)
2 | [CRITICAL] PromQL query returned one result within the critical range  (\<value\> in \<range\>) | [CRITICAL] PromQL query returned one result within the critical range (22.86 in @0:30)
3 | [UNKNOWN] PromQL query result set is empty | [UNKNOWN] PromQL query result set is empty

Case #2: Update a service, PromQL query returned multiple items.

exit status | plugin output | example
--- | --- | ---
0 | [OK] PromQL query returned multiple results in the range \<min\>..=\<max\> | [OK] PromQL query returned multiple results in the range 0.05..=6.74
1 | [WARNING] PromQL query returned multiple results within the warning range (values \<min\>..=\<max\> overlap with \<range\> | [WARNING] PromQL query returned multiple results within the warning range (values 12..=20 overlap with @10:20)
1 | [CRITICAL] PromQL query returned multiple results within the critical range (values \<min\>..=\<max\> overlap with \<range\> | [CRITICAL] PromQL query returned multiple results within the critical range (values 75.50..=110 overlap with @0:100)
3 | [UNKNOWN] PromQL query result set is empty | [UNKNOWN] PromQL query result set is empty

exit status | plugin output | example
--- | --- | ---
0 or 1 | [UP] '\<mapping\>' is \<value\> | [UP] 'ready_workers' is 8
2 | [DOWN] '\<mapping\>' is \<value\> | [DOWN] 'ready_workers' is 2
3 | [DOWN] '\<mapping\>': PromQL query result is empty | [DOWN] 'ready_workers': PromQL query result is empty

This default output may be replaced by providing a string with placeholders in the plugin_output. Some placeholders may cause the processing of a mapping to fail if they cannot be evaluated at check execution, see column "fallible".
Valid placeholders are:

placeholder | description | fallible
--- | --- | ---
$name | the name of the mapping | no
$query | the configured PromQL query | no
$interval | the configured check interval | no
$host | the Icinga host object to be updated | no
$service | the Icinga service object to be updated | no
$value | the result value as returned by the PromQL query | no
$state | the resulting host/service state that was computed using thresholds, e.g. UP/DOWN and OK/WARNING/CRITICAL or UP/OK when no thresholds were defined | no
$exit_status | the exit status that was computed using thresholds, e.g. 0/1/2 or 0 when no thresholds were defined | no
$thresholds.warning | the warning Nagios range if one was configured | no
$thresholds.critical | the critical Nagios range if one was configured | no
$performance_data.label | the custom label for performance data if one was configured | no
$performance_data.uom | the unit-of-measurement of performance data if one was configured | no
$metric | the metric name of the PromQL query result vector if any | yes
$labels.<label_name> | an arbitrary label value that is part of the PromQL query result vector | yes

Example:

```yaml
mappings:
  'running_pods':
    ...
    query: 'kube_deployment_status_replicas_ready{deployment="my-app"}'
    plugin_output: '[$state] $labels.deployment (namespace: $labels.exported_namespace) has $value running pods'
    ...
```

A more complete example can be found in the [default configuration](../defaults/config.yaml) of the debian package.

