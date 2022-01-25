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

At this point the Prometheus section is pretty slim.

```yaml
prometheus:
  # The URL at which the server is reachable in order to execute queries against the HTTP API.
  host: 'http://localhost:9090'
```