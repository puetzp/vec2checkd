# Configuration

## General

The default location of the configuration file  is `/etc/vec2checkd/config.yaml`. But another location can be provided via the `--config` flag. In the most general case nothing other than this file is required to get up and running.

The overall structure of the configuration file is pretty simple:

```yaml
---
prometheus: {}
icinga: {}
mappings: {}
```

The content of these sections is further explained below.

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
  # Default: 'https://localhost:5665'
  # Example:
  host: 'https://satellite1.icinga.local'

  # If no trust relationship between the system and the self-signed Icinga root certificate has been established by some means, the location of the certificate must be provided here.
  # Example:
  ca_cert: '/usr/local/share/ca-certificates/icinga.crt'
  
  authentication:
    # Valid authentication mechanims are HTTP Basic Auth and X.509.
    # Required.
    method: 'x509' | 'basic-auth'

    # Username and password for Basic authentication are only needed when method is set to 'basic-auth'.
    username: 'myuser'
    password: 'mypassword'

    # Paths to client certificate and key are only needed when method is set to 'x509'.
    # Make sure the files are owned/readable by the vec2check user.
    client_cert: '/var/lib/vec2checkd/ssl'
    client_key: '/var/lib/vec2checkd/ssl'
```
