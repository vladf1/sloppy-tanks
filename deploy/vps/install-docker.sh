# Sourced by install-runtime.sh when the servers run under Docker, the fallback runtime.
#
# daemon.json turns off Docker's bridge network and firewall rules: the servers use host
# networking, so Docker has no ports to publish around ufw. Its local log driver keeps
# container output bounded; the systemd journal also receives it through each unit.

# python3 parses /stats for auto-update; Ubuntu server images already ship it.
apt-get install -y docker.io python3
install -d -m 755 /etc/docker
if ! cmp -s daemon.json /etc/docker/daemon.json; then
  install -m 644 daemon.json /etc/docker/daemon.json
  # Restarting Docker stops running containers; their units start them again.
  systemctl restart docker
fi
# A switch to Podman disables Docker's daemons; switching back starts them again.
systemctl enable --now containerd docker
