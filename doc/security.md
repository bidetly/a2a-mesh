# Security

Each `a2a-mesh` process creates one **in-memory**, self-signed ECDSA P-256 X.509 identity at startup. The private key is neither written to disk nor accepted through configuration, command-line arguments, or logs. The certificate includes the advertised DNS name or IP address as a Subject Alternative Name (SAN). Its default lifetime is 366 days and is configurable with `security.certificate_lifetime_secs`; a five-minute clock-skew margin is added. The permitted maximum is 10 years.

The discovery pin is the lowercase hexadecimal SHA-256 digest of the certificate's RFC 5280 SubjectPublicKeyInfo (SPKI) DER. Later outbound transport wiring pins that exact value before sending A2A data; a different certificate will be rejected at TLS setup.

## A2A listener policy

Loopback A2A binds use HTTP by default. Set `security.a2a_tls = true` to use pinned TLS there. Every non-loopback or wildcard bind requires pinned TLS; there is no plaintext override.

Future outbound transport wiring will present the generated client certificate by default (`security.present_client_certificate = true`). Future servers will request, but not require, a client certificate: a recognized discovery fingerprint will be a verified mesh identity; absent or unknown client certificates will remain allowed as unverified non-mesh identities. Non-mesh clients establish trust out of band with the server certificate or its SPKI fingerprint.

## etcd

HTTPS etcd is expected outside loopback. A remote `http://` endpoint is rejected unless `security.allow_insecure_etcd = true`; accepting it emits exactly one startup warning. Loopback plaintext etcd is allowed for local development. etcd credentials are handled by the pinned `etcdrs` client integration, not a new a2a-mesh credential format.

Do not put credentials in endpoint URLs, command-line options, or logs. Endpoint URLs containing credentials are rejected.
