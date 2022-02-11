# vec2checkd

This program/daemon executes PromQL queries via the Prometheus HTTP API, translates the results to passive check results and sends them to the Icinga HTTP API in order to update certain host or service objects. These mappings (PromQL query result -> passive check result) are applied regularly per a user-defined interval, similar to an active service check applied by an Icinga satellite or agent.

The obvious choice to monitor anything that exports time-series data scraped by Prometheus et al. is Grafana and/or Alertmanager. However this little tool might come in handy when you/your organization relies primarily on Icinga2 for infrastructure monitoring but wants to integrate some services that export time-series data into the mix. In this case setting up Grafana and/or Alertmanager alongside Icinga2 may be overkill (or not ... I guess it "_depends_").

## Installation

Prerequisites: Rust >= v1.58.0

### .deb package

To install the .deb package either download from GitHub and

`dpkg -i <path_to_package>`

OR

Build it yourself by first installing the `cargo deb` command:

`cargo install cargo-deb`

and then building the package:

`cargo deb`

The .deb package also provides a systemd unit file and a default configuration file in `/etc/vec2checkd/config.yaml`.

### Binary only

`cargo install vec2checkd`

## Configuration

The systemd unit is designed to run multiple instances of vec2checkd using [templates](https://www.freedesktop.org/software/systemd/man/systemd.service.html#Service%20Templates). A new instance can be created like this:

```
$ ln -s -T /lib/systemd/system/vec2checkd@.service /etc/systemd/system/vec2checkd@<instance_name>.service
$ systemctl enable /etc/systemd/system/vec2checkd@<instance_name>.service
```

Each instance reads its configuration from `/etc/vec2checkd/conf.d/<instance_name>.yaml`. Though a custom location may be provided by overriding the settings in the unit file (the flag `--config` specifically). The expected content of each configuration is described [here](doc/configuration.md).

For simple use-cases a single instance of vec2checkd will suffice of course. The instantation of multiple daemons may help in some cases however, e.g. when:

* the "mappings" section of a single configuration becomes huge and better be split up into pieces.
* the queries in the "mappings" section target different systems (e.g. multiple K8s clusters) and may better be split up for clarity.
* different Prometheus servers are queried.
* passive check results are sent to different Icinga servers.

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

As per the defaults that come into play here, this mapping will execute the PromQL query every 60 seconds and send the following default plugin output and performance data to Icinga2 in order to update the service object "Failed ingress requests" on host "Kubernetes Production". The status will  be 0 (OK) as no thresholds have been defined.

```
# plugin output
[OK] 'Failed ingress requests' is 34.43

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

The second mapping will only be applied every 300 seconds. The warning and critical thresholds are also considered before the final check result is sent to Icinga2. Given the PromQL query evaluates to a value of "130.0", vec2checkd sends status 1 (WARNING) and the following plugin output and performance data to the API.

```
# plugin output
[WARNING] Nginx ingress controller processes 130 requests per second (HTTP 2xx)

# performance data
'requests'=130.0;@0:200;@0:100;;
```

There is a little more going on here, so check the [documentation](doc/configuration.md) about details on the placeholders in the plugin_output field, the thresholds, the performance_data object etc.

## Limitations

* In contrast to [signalilo](https://github.com/vshn/signalilo) vec2checkd is intended to interact with pre-defined host and service objects in Icinga2 and update those objects regularly. So **host and service objects are not created/deleted or managed in any way by vec2checkd** because Icinga2 provides excellent tools to create any type of object even in bulk, e.g. by using the [Director](https://github.com/Icinga/icingaweb2-module-director).
Providing a means to create objects would necessitate to re-create most of the logic that the Director already provides.
* At this point only the first item of a PromQL result vector is processed further and the result ultimately sent to Icinga2. So (for now) make sure that your PromQL query yields exactly one vector. *This might change in future versions*
* Only the PromQL result type "vector" is interpreted. *This might change in future versions*

## ToDos

* Extend the Prometheus configuration object with authentication options (as the server may be shielded by a reverse proxy).
* Extend the Prometheus and Icinga configuration objects with custom proxy options (setting HTTPS_PROXY and HTTP_PROXY in the environment would cause *both* clients to connect to Prometheus/Icinga2 via this proxy).
* Provide a means to process multiple result vectors when the PromQL query result type is "vector".
* Also provide a means to interpret a PromQL query result of type "matrix".
