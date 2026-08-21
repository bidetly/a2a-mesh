# Deployment

For a multi-host deployment, bind an explicit address, advertise the matching DNS name or IP address, enable A2A TLS, and use HTTPS etcd:

```toml
[listen]
address = "0.0.0.0"
advertised_host = "mesh-worker.internal"

[security]
a2a_tls = true
present_client_certificate = true

[etcd]
endpoints = ["https://etcd-a.internal:2379"]
```

Do not set `allow_insecure_etcd` in production. It exists only for explicitly acknowledged insecure environments and produces a startup warning when remote plaintext etcd is configured. Certificates and keys are process-ephemeral: restarting a process changes its pin, so registration/discovery must be refreshed before peers connect.
