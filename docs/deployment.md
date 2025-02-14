# Deployment

The [deployment](https://github.com/stackhpc/reductionist-rs/tree/main/deployment) directory in the Reductionist Git repository contains an Ansible playbook to deploy Reductionist and supporting services to one or more hosts.
The Ansible playbook allows for a secure, scale-out deployment of Reductionist, with an HAProxy load balancer proxying requests to any number of Reductionist backend servers.

The following services are supported:

* Podman engine
* Step CA Certificate Authority (generates certificates for Reductionist)
* Step CLI (requests and renews certificates)
* Minio object store (optional, for testing)
* Prometheus (monitors Reductionist and HAProxy)
* Jaeger (distributed tracing UI)
* Reductionist
* HAProxy (load balancer for Reductionist)

## Prerequisites

The existence of correctly configured hosts is assumed by this playbook.

The following host OS distributions have been tested and are supported:

* CentOS Stream 9
* Rocky Linux 9
* Ubuntu 24.04

Currently only a single network is supported.
Several TCP ports should be accessible on this network.
This may require configuration of a firewall on the host (e.g. firewalld, ufw) or security groups in a cloud environment.

* SSH: 22
* Reductionist backend: 8081
* Reductionist frontend: 8080 (HAProxy host only)
* Step CA: 9999 (Step CA host only)
* Minio: 9000 (Minio host only)
* Prometheus: 9090 (Prometheus host only)
* Jaeger: 16686 (Jaeger host only)

The Ansible control host (the host from which you will run `ansible-playbook`) should be able to resolve the hostnames of the hosts.
If names are not provided by DNS, entries may be added to `/etc/hosts` on the Ansible control host.
Issues have been reported when using Ansible with password-protected SSH private keys and SSH agent.

It may be desirable to host the Reductionist API on a different address, such as a hostname or public IP exposed on the host running HAProxy.
This may be configured using the `reductionist_host` variable.

## Configuration

An example Ansible inventory file is provided in [inventory](https://github.com/stackhpc/reductionist-rs/blob/main/deployment/inventory) which defines all groups and maps localhost to them. For a production deployment it is more typical to deploy to one or more remote hosts.

The following example inventory places HAProxy, Jaeger, Prometheus and Step CA on `reductionist1`, while Reductionist is deployed on `reductionist1` and `reductionist2`.

```ini
# Example inventory for deployment to two hosts, reductionist1 and reductionist2.

# HAProxy load balancer.
# Should contain exactly one host.
[haproxy]
reductionist1

# Jaeger distributed tracing UI.
# Should contain at most one host.
[jaeger]
reductionist1

# Prometheus monitoring service.
# Should contain at most one host.
[prometheus]
reductionist1

# Reductionist servers.
# May contain multiple hosts.
[reductionist]
reductionist[1:2]

# Step Certificate Authority (CA).
# Should contain exactly one host.
[step-ca]
reductionist1

# Do not edit.
[step:children]
reductionist

# Do not edit.
[podman:children]
haproxy
jaeger
minio
prometheus
reductionist
step-ca
```

Some variables are provided to configure the deployment in the [group_vars](https://github.com/stackhpc/reductionist-rs/tree/main/deployment/group_vars) directory. Reductionist configuration options may be specified using environment variables specified using `reductionist_env`.

## Ansible control host setup

Whether running Ansible on the same host as the Reductionist server(s) or a separate remote host, some setup is necessary.

Ensure Git and Pip are installed:
```sh
sudo apt -y install git python3-pip # Ubuntu
sudo dnf -y install git python3-pip # CentOS Stream or Rocky Linux
```

Clone the Reductionist source code:
```sh
git clone https://github.com/stackhpc/reductionist-rs
```

Change to the Reductionist source code directory:
```sh
cd reductionist-rs
```

When working with Pip it's generally best to install packages into a virtual environment, to avoid modifying the system packages.
```sh
python3 -m venv venv
source venv/bin/activate
```

## Installation

Install Python dependencies:
```sh
pip install -r deployment/requirements.txt
```

Install Ansible collections:
```sh
ansible-galaxy collection install -r deployment/requirements.yml
```

## Deployment

Podman will be used to run containers under the same user account used for ansible deployment.
To install requisite system packages some tasks will require sudo `privileged` access.

To run the entire playbook as a non-privileged user prompting for a sudo password:
```sh
ansible-playbook -i deployment/inventory deployment/site.yml -K
```

To run specific plays the following tags are supported and may be specified via `--tags <tag1,tag2>`:

* `podman` - runs privileged tasks to install packages
* `step-ca`
* `step` - runs privileged tasks to install and the CA certificate
* `minio`
* `prometheus`
* `jaeger`
* `reductionist`
* `haproxy`

### Minimal deployment of Podman and the Reductionist

Podman is a prerequisite for running the Reductionist.
Podman can run containers as an **non-privileged** user, however this user must have **linger** enabled on their account to allow Podman to continue to run after logging out of the user session.

To enable **linger** support for the non-privileged user:
```sh
sudo loginctl enable-linger <non-privileged user>
```

Alternatively, run the optional `podman` play to install Podman as an **non-privileged** user. The following will prompt for the sudo password to escalate privileges only for package installation and for enabling **linger** for the non-privileged user:
```sh
ansible-playbook -i deployment/inventory deployment/site.yml --tags podman -K
```

Then to run the `reductionist` play, again as the **non-privileged** user:
```sh
ansible-playbook -i deployment/inventory deployment/site.yml --tags reductionist
```

Podman containers require a manual restart after a system reboot.
This requires logging into the host(s) running the Reductionist as the **non-privileged** user to run:
```sh
podman restart reductionist
```

Automatic restart on boot can be enabled via **systemd**, not covered by this documentation.

### Using SSL/TLS certificates with the Reductionist

To enable **https** connections edit `deployment/group_vars/all` before deployment as set:

```
REDUCTIONIST_HTTPS: "true"
```

Note, this is the default.

Create a `certs` directory under the home directory of the non-privileged deployment user.
Ensure the following files are added to the this directory:

| Filename    | Description |
| -------- | ------- |
| certs/key.pem  | Private key file |
| certs/cert.pem | Certificate file including any intermediates |

Certificates can be added post Reductionist deployment but the Reductionist's container will need to be restarted afterwards.

## Usage

Once deployed, the Reductionist API is accessible on port 8080 by HAProxy. The Prometheus UI is accessible on port 9090 on the host running Prometheus. The Jaeger UI is accessible on port 16686 on the host running Jaeger.
