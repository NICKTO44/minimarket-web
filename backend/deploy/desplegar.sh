#!/bin/bash
# Despliegue Blue-Green del backend de Monspeet POS.
#
# Dos contenedores se turnan: minimarket-azul (127.0.0.1:3001) y
# minimarket-verde (127.0.0.1:3002). Nginx apunta a uno solo, definido en
# /etc/nginx/conf.d/monspeet-upstream.conf. El nuevo se levanta al lado del
# que atiende, se comprueba que responda y recién ahí Nginx cambia de uno a
# otro (reload sin cortar conexiones). El anterior queda DETENIDO pero
# guardado, para volver a él en segundos si algo sale mal.
#
# Uso (desde ~/minimarket-web/backend):
#   ./deploy/desplegar.sh v1.2.0      despliega esa versión (imagen de GitHub Actions)
#   ./deploy/desplegar.sh --rollback  vuelve al contenedor anterior
#   ./deploy/desplegar.sh --estado    cuál atiende y con qué versión
set -euo pipefail

IMAGEN="ghcr.io/nickto44/minimarket-backend"
DIR_BACKEND="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="$DIR_BACKEND/.env"
UPLOADS="/root/minimarket-uploads"
UPSTREAM_CONF="/etc/nginx/conf.d/monspeet-upstream.conf"
RED="monspeet"
MEMORIA="384m"     # hoy usa ~40 MB; el tope evita que una fuga tumbe a Chambeador/MySQL
ESPERA_SALUD=90    # segundos máximos esperando que el nuevo responda
DRENAJE=20         # segundos que el anterior sigue vivo después del cambio

puerto_de()        { case "$1" in azul) echo 3001;; verde) echo 3002;; legacy) echo 3000;; esac; }
color_de_puerto()  { case "$1" in 3001) echo azul;; 3002) echo verde;; *) echo legacy;; esac; }
contenedor_de()    { case "$1" in legacy) echo minimarket-backend;; *) echo "minimarket-$1";; esac; }
existe()           { docker inspect "$1" >/dev/null 2>&1; }
version_de()       { docker inspect "$1" --format '{{ index .Config.Labels "monspeet.version" }}' 2>/dev/null || true; }
puerto_activo()    { grep -oE '127\.0\.0\.1:[0-9]+' "$UPSTREAM_CONF" | head -1 | cut -d: -f2; }

esperar_salud() {
  local puerto=$1
  for _ in $(seq 1 "$ESPERA_SALUD"); do
    if curl -sf --max-time 2 "http://127.0.0.1:$puerto/" >/dev/null; then return 0; fi
    sleep 1
  done
  return 1
}

apuntar_nginx() {
  local puerto=$1
  cp "$UPSTREAM_CONF" "$UPSTREAM_CONF.anterior"
  cat > "$UPSTREAM_CONF" <<EOF
# Lo escribe deploy/desplegar.sh — no editar a mano.
upstream monspeet_backend {
    server 127.0.0.1:$puerto;
}
EOF
  if ! nginx -t -q; then
    echo "❌ nginx -t falló; se restaura el upstream anterior."
    cp "$UPSTREAM_CONF.anterior" "$UPSTREAM_CONF"
    return 1
  fi
  systemctl reload nginx
}

[ -f "$UPSTREAM_CONF" ] || { echo "❌ Falta $UPSTREAM_CONF (ver deploy/LEEME.md, configuración inicial)."; exit 1; }
[ -f "$ENV_FILE" ]      || { echo "❌ Falta $ENV_FILE"; exit 1; }

P_ACTIVO=$(puerto_activo)
C_ACTIVO=$(color_de_puerto "$P_ACTIVO")
N_ACTIVO=$(contenedor_de "$C_ACTIVO")

case "${1:-}" in
  --estado)
    echo "Atiende: $N_ACTIVO (127.0.0.1:$P_ACTIVO) versión: $(version_de "$N_ACTIVO")"
    docker ps -a --filter "name=minimarket-" --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.Image}}'
    exit 0
    ;;

  --rollback)
    # El anterior es el otro color; si no existe, el contenedor viejo (puerto 3000).
    if [ "$C_ACTIVO" = azul ]; then C_ANT=verde; else C_ANT=azul; fi
    existe "$(contenedor_de "$C_ANT")" || C_ANT=legacy
    N_ANT=$(contenedor_de "$C_ANT"); P_ANT=$(puerto_de "$C_ANT")
    existe "$N_ANT" || { echo "❌ No hay contenedor anterior al que volver."; exit 1; }
    echo "==> Rollback: $N_ACTIVO -> $N_ANT ($(version_de "$N_ANT"))"
    docker start "$N_ANT" >/dev/null
    esperar_salud "$P_ANT" || { echo "❌ $N_ANT no responde. Producción sigue en $N_ACTIVO."; exit 1; }
    apuntar_nginx "$P_ANT"
    echo "==> Nginx ya apunta a $N_ANT. Drenando $DRENAJE s antes de detener $N_ACTIVO..."
    sleep "$DRENAJE"
    docker stop --time 30 "$N_ACTIVO" >/dev/null
    echo "✅ Rollback terminado."
    exit 0
    ;;

  ""|-*)
    echo "Uso: $0 <version> | --rollback | --estado"; exit 1
    ;;
esac

VERSION="$1"
if [ "$C_ACTIVO" = azul ]; then C_NUEVO=verde; else C_NUEVO=azul; fi
N_NUEVO=$(contenedor_de "$C_NUEVO"); P_NUEVO=$(puerto_de "$C_NUEVO")

echo "==> Atiende ahora: $N_ACTIVO (:$P_ACTIVO). La versión $VERSION se levantará en $N_NUEVO (:$P_NUEVO)."

echo "==> Descargando imagen $IMAGEN:$VERSION..."
docker pull "$IMAGEN:$VERSION"

# El contenedor de ese color, si existe, es el respaldo de hace dos despliegues.
docker rm -f "$N_NUEVO" >/dev/null 2>&1 || true

echo "==> Levantando $N_NUEVO..."
docker run -d \
  --name "$N_NUEVO" \
  --restart unless-stopped \
  --network "$RED" \
  --env-file "$ENV_FILE" \
  --memory "$MEMORIA" \
  -p "127.0.0.1:$P_NUEVO:3000" \
  -v "$UPLOADS:/app/uploads" \
  --label "monspeet.version=$VERSION" \
  "$IMAGEN:$VERSION" >/dev/null

MONTAJE=$(docker inspect "$N_NUEVO" --format '{{range .Mounts}}{{.Source}}{{end}}')
if [ "$MONTAJE" != "$UPLOADS" ]; then
  echo "❌ El volumen de uploads no quedó montado (se vio: '$MONTAJE'). Producción no se tocó."
  docker rm -f "$N_NUEVO" >/dev/null; exit 1
fi

echo "==> Esperando que $N_NUEVO responda (hasta $ESPERA_SALUD s)..."
if ! esperar_salud "$P_NUEVO"; then
  echo "❌ $N_NUEVO no respondió. Últimas líneas del log:"
  docker logs --tail 40 "$N_NUEVO" || true
  docker rm -f "$N_NUEVO" >/dev/null
  echo "==> Producción no se tocó: sigue atendiendo $N_ACTIVO."
  exit 1
fi

echo "==> Cambiando Nginx a $N_NUEVO..."
apuntar_nginx "$P_NUEVO" || { docker rm -f "$N_NUEVO" >/dev/null; exit 1; }

echo "==> Drenando $DRENAJE s para que $N_ACTIVO termine lo que tenga en curso..."
sleep "$DRENAJE"
if existe "$N_ACTIVO"; then docker stop --time 30 "$N_ACTIVO" >/dev/null; fi

echo "✅ Desplegado $VERSION en $N_NUEVO. $N_ACTIVO quedó detenido como respaldo."
echo "   Si algo falla:  ./deploy/desplegar.sh --rollback"
