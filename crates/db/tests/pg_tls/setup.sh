#!/usr/bin/env bash
# Throwaway PostgreSQL servers for crates/db/tests/pg_tls.rs: certificates, three clusters, pg_hba rules.
#
#   crates/db/tests/pg_tls/setup.sh start <dir>   create and start the servers, print the DQD_PG_* variables
#   crates/db/tests/pg_tls/setup.sh stop  <dir>   stop them
#
# Needs openssl and the PostgreSQL server binaries (set PGBIN, default /usr/lib/postgresql/*/bin newest).
# PostgreSQL refuses to run as root, so when run as root the servers run as the `postgres` user.
# Servers (all on 127.0.0.1, user `dqd`/`secret`, database `postgres`):
#   good      TLS, certificate for localhost/127.0.0.1 issued by ca.crt; also `sslonly` (rejects plain
#             connections) and `certuser` (client certificate authentication)
#   wrongname TLS, certificate for other.example only (issued by ca.crt)
#   plain     no TLS
set -euo pipefail
action=${1:?start|stop}
dir=$(realpath -m "${2:?directory}")
PGBIN=${PGBIN:-$(ls -d /usr/lib/postgresql/*/bin | sort -V | tail -1)}
GOOD=${DQD_PG_TLS_PORT:-54431}
WRONG=${DQD_PG_TLS_WRONGNAME_PORT:-54432}
PLAIN=${DQD_PG_PLAIN_PORT:-54433}

as_pg() { if [ "$(id -u)" = 0 ]; then runuser -u postgres -- "$@"; else "$@"; fi; }

if [ "$action" = stop ]; then
  for n in good wrongname plain; do
    [ -d "$dir/$n" ] && as_pg "$PGBIN/pg_ctl" stop -D "$dir/$n" -m fast >/dev/null 2>&1 || true
  done
  exit 0
fi

mkdir -p "$dir"
cd "$dir"
if [ ! -f ca.crt ]; then
  mk_cert() { # name subject san
    openssl req -newkey rsa:2048 -nodes -keyout "$1.key" -subj "/CN=$2" -out "$1.csr" 2>/dev/null
    printf 'subjectAltName=%s\nextendedKeyUsage=%s\n' "$3" "$4" >"$1.ext"
    openssl x509 -req -in "$1.csr" -CA ca.crt -CAkey ca.key -CAcreateserial -days 3 -out "$1.crt" -extfile "$1.ext" 2>/dev/null
  }
  openssl req -x509 -newkey rsa:2048 -nodes -keyout ca.key -subj "/CN=dqd test CA" -days 3 -out ca.crt 2>/dev/null
  openssl req -x509 -newkey rsa:2048 -nodes -keyout other-ca.key -subj "/CN=unrelated CA" -days 3 -out other-ca.crt 2>/dev/null
  mk_cert good localhost "DNS:localhost,IP:127.0.0.1" serverAuth
  mk_cert wrongname other.example "DNS:other.example" serverAuth
  mk_cert client certuser "DNS:certuser" clientAuth
  openssl pkcs8 -topk8 -nocrypt -in client.key -outform DER -out client.pk8
  openssl pkcs8 -topk8 -v1 PBE-SHA1-3DES -passout pass:pw -in client.key -out client-encrypted.key
fi
chmod 644 *.crt *.pk8 client*.key
chmod 600 good.key wrongname.key
[ "$(id -u)" = 0 ] && { chown -R postgres: "$dir"; }

init() { # name port ssl(on|off) cert
  local d="$dir/$1"
  if [ ! -d "$d" ]; then
    echo secret >"$dir/pw"; [ "$(id -u)" = 0 ] && chown postgres: "$dir/pw"
    as_pg "$PGBIN/initdb" -D "$d" -U dqd --pwfile="$dir/pw" --auth-host=scram-sha-256 --auth-local=trust -E UTF8 >/dev/null
    {
      echo "listen_addresses = '127.0.0.1'"
      echo "port = $2"
      echo "unix_socket_directories = '$d'"
      echo "ssl = $3"
      if [ "$3" = on ]; then
        echo "ssl_cert_file = '$dir/$4.crt'"
        echo "ssl_key_file = '$dir/$4.key'"
        echo "ssl_ca_file = '$dir/ca.crt'"
      fi
    } >>"$d/postgresql.conf"
    if [ "$1" = good ]; then
      {
        echo "hostnossl all sslonly 127.0.0.1/32 reject"
        echo "hostssl all sslonly 127.0.0.1/32 scram-sha-256"
        echo "hostssl all certuser 127.0.0.1/32 cert clientcert=verify-full"
      } | cat - "$d/pg_hba.conf" >"$d/pg_hba.new" && mv "$d/pg_hba.new" "$d/pg_hba.conf"
      [ "$(id -u)" = 0 ] && chown postgres: "$d/pg_hba.conf"
    fi
  fi
  as_pg "$PGBIN/pg_ctl" start -D "$d" -l "$d/log" -w >/dev/null
}
init good "$GOOD" on good
init wrongname "$WRONG" on wrongname
init plain "$PLAIN" off ""

for role in sslonly certuser; do
  as_pg "$PGBIN/psql" -h "$dir/good" -p "$GOOD" -U dqd -d postgres -qc "DO \$\$ BEGIN IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname='$role') THEN CREATE ROLE $role LOGIN PASSWORD 'secret'; END IF; END \$\$"
done
cat <<VARS
export DQD_PG_TLS_DIR=$dir
export DQD_PG_TLS_PORT=$GOOD
export DQD_PG_TLS_WRONGNAME_PORT=$WRONG
export DQD_PG_PLAIN_PORT=$PLAIN
VARS
