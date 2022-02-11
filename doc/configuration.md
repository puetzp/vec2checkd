# Configuration

## General

As was mentioned in the README each instance of vec2checkd by default reads its configuration file from `/etc/vec2checkd/conf.d/<instance_name>.yaml`. But another location can be provided via the `--config` flag. In the most general case nothing other than this file is required to get up and running.

The overall structure of the configuration file is pretty simple:

```yaml
---
# Parameters needed for the Prometheus API client.
prometheus: {}

# Parameters needed for the Icinga API client.
icinga: {}

# Set of configurations that map PromQL query results to Icinga (passive) check results.
mappings: {}
```

The content of these sections is further explained below.

First of all some clarifications how PromQL query results are mapped to passive Icinga check results:

* The "exit status", that is the Icinga host/service state is determined by comparing the PromQL query result value with warning and critical thresholds.
* The passive check result is used to either update an Icinga host object (when _only_ the host name was provided in the mapping) or a service object (when _both_ the host name and service name were provided).

### Prometheus

At this point the Prometheus section is rather slim.

```yaml
prometheus:
  # The URL at which the server is reachable in order to execute queries against the HTTP API.
  # Default: 'http://localhost:9090'
  # Example:
  host: 'https://prometheus.example.com:9090'
```

### Icinga

The Icinga API is more difficult to set up in that it _requires_ HTTPS and either HTTP Basic Auth or x509 authentication. Please refer to the [Icinga documentation](https://icinga.com/docs/icinga-2/latest/doc/12-icinga2-api/) in order to set up the API and a user object with an adequate set of permissions.

```yaml
icinga:
  # The URL at which the server is reachable in order to execute queries against the HTTP API. In this case HTTPS is required.
  # OPTIONAL, default: 'https://localhost:5665'
  # Example:
  host: 'https://satellite1.icinga.local'

  # If no trust relationship between the system and the self-signed Icinga root certificate has been established by some means, the location of the certificate must be provided here.
  # OPTIONAL.
  # Example:
  ca_cert: '/usr/local/share/ca-certificates/icinga.crt'
  
  authentication:
    # Valid authentication mechanims are HTTP Basic Auth and X.509.
    # REQUIRED.
    method: 'x509' | 'basic-auth'

    # Username and password for Basic authentication are only needed when method is set to 'basic-auth'.
    username: 'myuser'
    password: 'mypassword'

    # Paths to client certificate and key are only needed when method is set to 'x509'.
    # Make sure the files are owned/readable by the vec2check user.
    client_cert: '/var/lib/vec2checkd/ssl'
    client_key: '/var/lib/vec2checkd/ssl'
```

Note that the Icinga ApiUser username and password (Basic auth.) may also be read from the environment using the variables **V2C_ICINGA_USERNAME** and **V2C_ICINGA_PASSWORD** respectively. When the username and password are defined in both the environment and the configuration file, the values from the environment take precedence over the YAML.

### Mappings

As hinted above a "mapping" defines PromQL queries to execute regularly and how to map the result of those queries to passive check results that are ultimately sent to the Icinga HTTP API.
Note that the daemon will simply exit when no mappings are configured.

```yaml
mappings:
  # Give each mapping a descriptive name as you would to a Prometheus recording or alerting rule.
  '<name>':
    # Probably self-explanatory. Specify a PromQL query to execute against the HTTP API.
    # REQUIRED.
    query: '<promql_query>'

    # The name of the Icinga host object to be updated.
    # REQUIRED.
    host: '<host_object'

    # The name of the Icinga service object to be updated.
    # Note: When this is omitted the passive check result will update the host object instead.
    # OPTIONAL.
    service: '<service_object>'

    # Check interval or how often á mapping is processed in seconds. Must be in the range 10..=3600.
    # OPTIONAL, default 60.
    interval: <check_interval_in_seconds>

    # Use warning and critical thresholds to check the PromQL query result value and determine the Icinga host/service state.
    # Each threshold must be a Nagios range.
    # OPTIONAL.
    thresholds:
      # OPTIONAL.
      warning: '<nagios_range>'

      # OPTIONAL.
      critical: '<nagios_range>'

    # Used to customize the "plugin output" (in nagios-speak) when the default output does not suffice.
    # This is further explained below.
    # OPTIONAL.
    plugin_output: '<custom_output>'

    # Define if and how to send performance data as part of a passive check result.
    # OPTIONAL.
    performance_data:
      # Choose if performance data is sent.
      # OPTIONAL, default true.
      enabled: true|false

      # Customize the performance data label if desired.
      # OPTIONAL.
      label: '<custom_label>'

      # Customize the unit of measurement if desired.
      # OPTIONAL.
      uom: '<custom_unit_of_measurement>'
```

**NOTE:** _Nagios ranges_ and _UOMs_ are explained in the [Nagios development guidelines](https://nagios-plugins.org/doc/guidelines.html#THRESHOLDFORMAT).

#### Details on plugin output

The default _plugin output_ is as follows for service objects:

exit status | plugin output | example
--- | --- | ---
0 | [OK] '\<mapping\>' is \<value\> | [OK] 'running_pods' is 5
1 | [WARNING] '\<mapping\>' is \<value\> | [WARNING] 'running_pods' is 3
2 | [CRITICAL] '\<mapping\>' is \<value\> | [CRITICAL] 'running_pods' is 1
3 | [UNKNOWN] '\<mapping\>': PromQL query result is empty | [UNKNOWN] 'running_pods': PromQL query result is empty

... and for host objects:

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

#### Details on performance data

The default _performance data_ that is sent as part of a passive check result has the following format:

```
'<label>'=<value><uom>;<warn>;<crit>;;

# Example:

'ready_pods'=15;@5:10;@5;;
```

The warning and critical thresholds are only inserted into the performance data string if they were configured in the mapping. Min and max values are not represented. The name (or "label") of the performance data string is single-quoted as it may contain whitespace. **By default the label matches the mapping name**. The label may also be customized, which can be desirable if the mapping name happens to be quite wordy and you want to keep the performance data label clear and concise. A custom unit of measurement (UOM) may also be provided. See the following examples:

```yaml
# Given the mapping:
mappings:
  'Successful ingress requests (Test)':
    query: 'sum(rate(nginx_ingress_controller_requests{cluster="test",status=~"2.."}[5m]))'
    host: 'Kubernetes (Test-Cluster)'
    service: 'Successful ingress requests'
    interval: 300
    thresholds:
      critical: '@500'
    plugin_output: '[$state] Nginx ingress controller processes $value requests per second (HTTP 2xx)'
    # Note that 'enabled' is true by default.
    performance_data: {}

# ... the performance data string looks like:

'Successful ingress requests (Test)'=20;;@0:500;;

# Now configure a custom label:
mappings:
  'Successful ingress requests (Test)':
    query: 'sum(rate(nginx_ingress_controller_requests{cluster="test",status=~"2.."}[5m]))'
    host: 'Kubernetes (Test-Cluster)'
    service: 'Successful ingress requests'
    interval: 300
    thresholds:
      critical: '@500'
    plugin_output: '[$state] Nginx ingress controller processes $value requests per second (HTTP 2xx)'
    # Note that 'enabled' is true by default.
    performance_data:
      label: 'requests'

# ... and the performance data string looks like:

'requests'=20;;@0:500;;
```

**NOTE:** The syntax of performance data (including thresholds and UOM) is defined in the [Nagios development guidelines](https://nagios-plugins.org/doc/guidelines.html#AEN200).

