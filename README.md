# avi-metrics-exporter
Simple prometheus exporter for AVI Networks

### Introduction:

You can either build the binary locally, or run as a [container](https://hub.docker.com/r/findelabs/avi-metrics-exporter):

Build locally, after installing [rust](https://rustup.rs/):
```
cargo install --git https://github.com/findelabs/avi-metrics-exporter.git
```


### Usage

```
USAGE:
    avi-metrics-exporter [OPTIONS] --config <FILE> --controller <AVI_CONTROLLER> --password <AVI_PASSWORD> --username <AVI_USERNAME>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -k, --accept-invalid <accept_invalid>    Accept invalid certs from the connect cluster [env: ACCEPT_INVALID=] [default: false]
    -c, --config <FILE>                      Config file
    -c, --controller <AVI_CONTROLLER>        AVI Controller [env: AVI_CONTROLLER=]
    -p, --password <AVI_PASSWORD>            AVI Password [env: AVI_PASSWORD=]
    -p, --port <port>                        Set port to listen on [env: LISTEN_PORT=]  [default: 8080]
    -t, --threads <threads>                  Concurrent background threads to use when get'ing metrics [env: BACKGROUND_THREADS=] [default: 4]
    -u, --username <AVI_USERNAME>            AVI Username [env: AVI_USERNAME=]
```

### Example Config

```
/api/analytics/prometheus-metrics/virtualservice:
  entity_name: []
  tenant: ["argocd@np","metrics-monitoring@np"]

/api/analytics/prometheus-metrics/serviceengine:
  entity_name: ['k8snode1--se','k8snode2--se','k8snode3--se']
  tenant: []
```

### Endpoints 

```
/metrics:        Gets back all metrics as specified in config file
/login:          Attempts to reauth to AVI controller
/expires:        Shows the time that the login cookie will expire
/config:         Returns the current config in use
/refresh_config: Read in config from disk
/health:         Returns back if app is healthy or not
```
