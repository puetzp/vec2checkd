# vec2checkd

This program/daemon executes PromQL queries via the Prometheus HTTP API, translates the results to passive check results and sends them to the Icinga HTTP API in order to update certain host or service objects. These mappings (PromQL query result -> passive check result) are applied regularly per a user-defined interval, similar to an active service check applied by an Icinga satellite or agent.

The obvious choice to monitor anything that exports time-series data scraped by Prometheus et al. is Grafana and/or Alertmanager. However this little tool might come in handy when you/your organization relies primarily on Icinga2 for infrastructure monitoring but wants to integrate some services that export time-series data into the mix. In this case setting up Grafana and/or Alertmanager alongside Icinga2 may be overkill (or not ... I guess it "_depends_").

## Limitations

* In contrast to [signalilo](https://github.com/vshn/signalilo) vec2checkd is intended to interact with pre-defined host and service objects in Icinga2 and update those objects regularly. So **host and service objects are not created/deleted or managed in any way by vec2checkd** because Icinga2 provides excellent tools to create any type of object even in bulk, e.g. by using the [Director](https://github.com/Icinga/icingaweb2-module-director).
Providing a means to create objects would necessitate to re-create most of the logic that the Director already provides.
* At this point only the first item of a PromQL result vector is processed further and the result ultimately sent to Icinga2. So (for now) make sure that your PromQL query yields exactly one vector. *This change in future versions*
* Only the PromQL result type "vector" is interpreted. *This might change in future versions*

## Installation

==Note that a pre-built .deb package will be provided in the future.==

Prerequisites: rustc >= v1.58.0 and cargo >= v1.58.0

### .deb package

If you want to build and install vec2checkd from a .deb package, first install the `cargo deb` command:

`cargo install cargo-deb`

Then build the project:

`cargo deb`

### Binary only

You probably know the drill:

`cargo build --release`

## Configuration

See the [documentation on configuration](doc/configuration.md).




