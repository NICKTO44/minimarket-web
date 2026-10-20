# Despliegue del backend (Blue-Green)

## Flujo normal
1. En la Mac: commit y push a `main` como siempre (esto NO despliega).
2. Cuando quieras sacar versión: `git tag v1.2.0 && git push origin v1.2.0`.
   GitHub Actions construye la imagen (pestaña Actions) y la publica en
   `ghcr.io/nickto44/minimarket-backend:v1.2.0`. El Droplet no compila nada.
3. En el servidor: `cd ~/minimarket-web/backend && git pull && ./deploy/desplegar.sh v1.2.0`
4. Si algo sale mal: `./deploy/desplegar.sh --rollback` (vuelve en segundos).
5. Ver qué atiende: `./deploy/desplegar.sh --estado`
6. Comandos dentro del contenedor activo: `./deploy/mm ./licencias ...`, `./deploy/mm ./migrar`

## Regla de migraciones
Durante el cambio conviven la versión vieja y la nueva sobre las mismas
bases, y un rollback vuelve a código viejo con el esquema nuevo. Por eso
cada migración debe ser ADITIVA (agregar tablas/columnas). Renombrar, borrar
o reconstruir una tabla se hace en dos versiones: primero agregar lo nuevo,
y en una versión posterior quitar lo viejo.

## Configuración inicial (una sola vez, en el servidor)
1. Login en GHCR con un token de GitHub (classic, permiso `read:packages`):
   `docker login ghcr.io -u NICKTO44`
2. Upstream de Nginx (al inicio apunta al contenedor actual, puerto 3000):
   `cp deploy/nginx/monspeet-upstream.conf /etc/nginx/conf.d/`
3. Sitio de Nginx sin el bloque duplicado:
   `cp /etc/nginx/sites-available/minimarket-backend /root/minimarket-backend.nginx.bak`
   `cp deploy/nginx/minimarket-backend /etc/nginx/sites-available/minimarket-backend`
   `nginx -t && systemctl reload nginx`
4. Primer despliegue: `./deploy/desplegar.sh v1.0.0` (pasa de minimarket-backend
   a minimarket-azul; el viejo queda detenido como respaldo).
