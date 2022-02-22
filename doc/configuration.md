# Configuration

## General

Each instance of vec2checkd by default reads its configuration file from `/etc/vec2checkd/conf.d/<instance_name>.yaml`. But another location can be provided via the `--config` flag.

The overall structure of the configuration file is simple:

```yaml
---
# Parameters needed for the Prometheus API client.
prometheus: {}

# Parameters needed for the Icinga API client.
icinga: {}

# Set of configurations that map PromQL query results to Icinga (passive) check results.
mappings: {}
```

The content of each section is further explained below.

First of all some clarifications how PromQL query results are mapped to passive Icinga check results:

* The overall "exit status", that is the Icinga host/service state is determined by comparing the value of each vector in the PromQL query result set with warning and critical thresholds. The most significant individual status determines the overall status (e.g. when ONE value lies within a critical threshold, the overall exit status is => "DOWN" / "CRITICAL").
* A passive check result can update eiher an Icinga host object (when _only_ the host name was provided in the mapping) or a service object (when _both_ the host name and service name were provided).

### Prometheus

At this point the Prometheus section only allows setting the `host` to send PromQL queries to.

```yaml
prometheus:
  # The URL at which the server is reachable in order to execute queries against the HTTP API.
  # Default: 'http://localhost:9090'
  # Example:
  host: 'https://prometheus.example.com:9090'
```

### Icinga

The Icinga API is more difficult to set up in that it _requires_ HTTPS and either HTTP Basic Auth or x509 authentication. Please refer to the [Icinga documentation](https://icinga.com/docs/icinga-2/latest/doc/12-icinga2-api/) in order to set up the API and a user object with an adequate set of permissions.
Once the API is set up, configure the required parameters in the `icinga` section.

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

Note that the Icinga ApiUser username and password (Basic auth.) may also be read from the environment using the variables **V2C_ICINGA_USERNAME** and **V2C_ICINGA_PASSWORD** respectively. When the username and password are defined in both the environment and the configuration file, the values from the environment take precedence over the YAML parameters.

### Mappings

A "mapping" defines a PromQL query to be executed and how to map the query result to a passive check result that is ultimately sent to the Icinga HTTP API.

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

Some of these parameters are further explained in the following sections:

* On valid **Nagios ranges** see [Nagios development guidelines](https://nagios-plugins.org/doc/guidelines.html#THRESHOLDFORMAT) or [Icinga2 documentation](https://icinga.com/docs/icinga-2/latest/doc/05-service-monitoring/#threshold-ranges)
* On **plugin output** and customization see [this document](plugin_output.md)
* On **performance data** and customization see [this document](performance_data.md)

