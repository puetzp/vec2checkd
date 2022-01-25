# Configuration

## General

The default location of the configuration file  is `/etc/vec2checkd/config.yaml`. But another location can be provided via the `--config` flag. In the most general case nothing other than this file is required to get up and running. Since Icinga _requires_ authentication a client key and certificate may also be needed. These are generally stored in `/var/lib/vec2checkd/ssl` which is also pre-installed via the `.deb` package.

The overall structure of the configuration file is pretty simple:

```yaml
---
prometheus: {}
icinga: {}
mappings: {}
```

The content of these sections is further explained below.