# vec2checkd

This program/daemon executes PromQL queries via the Prometheus HTTP API, translates the results to passive check results and sends them to the Icinga HTTP API in order to update certain host or service objects. These mappings (PromQL query result -> passive check result) are applied regularly per a user-defined interval, similar to an active service check applied by an Icinga satellite or agent.

The obvious choice to monitor anything that exports time-series data scraped by Prometheus et al. is Grafana and/or Alertmanager. However this little tool might come in handy when you/your organization relies primarily on Icinga2 for infrastructure monitoring but wants to integrate some services that export time-series data into the mix. In this case setting up Grafana and/or Alertmanager alongside Icinga2 may be overkill (or not ... I guess it "_depends_").

## Limitations

* In contrast to [signalilo](https://github.com/vshn/signalilo) vec2checkd is intended to interact with pre-defined host and service objects in Icinga2 and update those objects regularly. So **host and service objects are not created/deleted or managed in any way by vec2checkd** because Icinga2 provides excellent tools to create any type of object even in bulk, e.g. by using the [Director](https://github.com/Icinga/icingaweb2-module-director).
Providing a means to create objects would necessitate to re-create most of the logic that the Director already provides.
* At this point only the first item of a PromQL result vector is processed further and the result ultimately sent to Icinga2. So (for now) make sure that your PromQL query yields exactly one vector. *This change in future versions*
* Only the PromQL result type "vector" is interpreted. *This might change in future versions*

## Installation

First know that a pre-built .deb package will be provided in the future!

Prerequisites: rustc >= v1.58.0 and cargo >= v1.58.0

### .deb package

If you want to build and install vec2checkd from a .deb package, first install the `cargo deb` command:

`cargo install cargo-deb`

Then build the project:

`cargo deb`

The .deb package also provides a systemd unit file and a default configuration file in `/etc/vec2checkd/config.yaml`.

### Binary only

You probably know the drill:

`cargo build --release`

## Configuration

vec2checkd reads its configuration from a single YAML file (default is `/etc/vec2checkd/config.yaml`. A custom location may be provided using the `--config` command-line argument. See the [documentation on configuration](doc/configuration.md) for a detailed explanation on the contents of the YAML file.

### Examples

Below is an example configuration that starts out pretty simple relying primarily on defaults set by vec2checkd.

```yaml
# Connects to 'http://localhost:9090' by default.
prometheus: {}
icinga:
  host: 'https://my-satellite.exmaple.com:5665'
  authentication:
    method: 'x509'
    client_cert: '/var/lib/vec2checkd/ssl/kubernetes-monitoring.crt'
    client_key: '/var/lib/vec2checkd/ssl/kubernetes-monitoring.key'
mappings:
  'Failed ingress requests':
    query: 'sum(rate(nginx_ingress_controller_requests{cluster="production",status!~"2.."}[5m]))'
    host: 'Kubernetes Production'
    service: 'Failed ingress requests'

  ...
```

As per the defaults that come into play here, this mapping will execute the PromQL query every 60 seconds and send the following default plugin output and performance data to Icinga2 in order to update the service object "Failed ingress requests" on host "Kubernetes Production" with status 0 (OK) as no thresholds have been defined.

```
# plugin output
[OK] 'Failed ingress requests' is 34.4393348197696023

# performance data
'Failed ingress requests'=34.4393348197696023;;;;
```

Now extend the configuration with another example mapping that builts on the existing one.

```yaml
prometheus: {}
icinga:
  host: 'https://my-satellite.exmaple.com:5665'
  authentication:
    method: 'x509'
    client_cert: '/var/lib/vec2checkd/ssl/kubernetes-monitoring.crt'
    client_key: '/var/lib/vec2checkd/ssl/kubernetes-monitoring.key'
mappings:
  'Failed ingress requests':
    query: 'sum(rate(nginx_ingress_controller_requests{cluster="production",status!~"2.."}[5m]))'
    host: 'Kubernetes Production'
    service: 'Failed ingress requests'

  'Successful ingress requests':
    query: 'sum(rate(nginx_ingress_controller_requests{cluster="production",status=~"2.."}[5m]))'
    host: 'Kubernetes Production'
    service: 'Successful ingress requests'
    interval: 300
    # In words: "WARN if the value dips below 200 or CRIT when the value dips below 100".
    thresholds:
      warning: '@200'
      critical: '@100'
    plugin_output: '[$state] Nginx ingress controller processes $value requests per second (HTTP 2xx)'
    performance_data:
      enabled: true
      label: 'requests'

  ...
```

The second mapping will only be applied every 300 seconds alongside the first one. The warning and critical thresholds are also considered before the final check result is sent to Icinga2. Given the PromQL query evaluates to a value of "130.0", vec2checkd sends status 1 (WARNING) and the following plugin output and performance data to the API.

```
# plugin output
[WARNING] Nginx ingress controller processes 150.0 requests per second (HTTP 2xx)

# performance data
'requests'=150.0;@0:200;@0:100;;
```

There is a little more going on here, so check the [documentation](doc/configuration.md) about details on the placeholders in the plugin_output field, the thresholds, the performance_data object etc.


