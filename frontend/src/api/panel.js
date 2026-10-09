// API del panel de Monspeet (el dueño del sistema). Sesión propia, separada
// de la de los negocios: otro token, guardado con otra clave.
import { API_URL } from './api';

const CLAVE_SESION = 'monspeet_panel_sesion';

export function sesionPanel() {
  try {
    return JSON.parse(localStorage.getItem(CLAVE_SESION) || 'null');
  } catch {
    return null;
  }
}

export function guardarSesionPanel(sesion) {
  try {
    if (sesion) localStorage.setItem(CLAVE_SESION, JSON.stringify(sesion));
    else localStorage.removeItem(CLAVE_SESION);
  } catch {
    // sin almacenamiento: la sesión dura lo que dure la pestaña
  }
}

/** Se dispara cuando la sesión del panel vence, para volver al login. */
export const EVENTO_SESION_VENCIDA = 'panel-sesion-vencida';

async function leer(res) {
  const esJson = res.headers.get('content-type')?.includes('application/json');
  return esJson ? res.json().catch(() => null) : res.text();
}

async function pedir(ruta, { metodo = 'GET', cuerpo, formulario } = {}) {
  const headers = {};
  const token = sesionPanel()?.token;
  if (token) headers.Authorization = `Bearer ${token}`;
  if (cuerpo !== undefined) headers['Content-Type'] = 'application/json';

  let res;
  try {
    res = await fetch(`${API_URL}${ruta}`, {
      method: metodo,
      headers,
      body: formulario ?? (cuerpo !== undefined ? JSON.stringify(cuerpo) : undefined),
    });
  } catch {
    throw new Error('No se pudo conectar con el backend. ¿Está corriendo?');
  }
  const datos = await leer(res);
  if (res.status === 401 && ruta !== '/panel/login') {
    guardarSesionPanel(null);
    window.dispatchEvent(new Event(EVENTO_SESION_VENCIDA));
    throw new Error('Tu sesión del panel venció. Vuelve a entrar.');
  }
  if (!res.ok) {
    const mensaje = typeof datos === 'string' && datos ? datos : datos?.message;
    throw new Error(mensaje || `Error ${res.status}`);
  }
  return datos;
}

export const panelApi = {
  login: (usuario, clave) => pedir('/panel/login', { metodo: 'POST', cuerpo: { usuario, clave } }),

  negocios: () => pedir('/panel/negocios'),
  renovar: (id, cantidad, unidad) => pedir(`/panel/negocios/${id}/renovar`, { metodo: 'POST', cuerpo: { cantidad, unidad } }),
  cambiarEstado: (id, estado) => pedir(`/panel/negocios/${id}/estado`, { metodo: 'POST', cuerpo: { estado } }),
  fijarVencimiento: (id, fecha) => pedir(`/panel/negocios/${id}/vencimiento`, { metodo: 'PUT', cuerpo: { fecha } }),

  codigos: () => pedir('/panel/codigos'),
  generarCodigo: (cantidad, unidad, dias_para_caducar) =>
    pedir('/panel/codigos', { metodo: 'POST', cuerpo: { cantidad, unidad, dias_para_caducar } }),

  facturacion: (id) => pedir(`/panel/negocios/${id}/facturacion`),
  guardarDatos: (id, datos) => pedir(`/panel/negocios/${id}/facturacion/datos`, { metodo: 'PUT', cuerpo: datos }),
  darDeAlta: (id, { archivo, claveCertificado, usuarioSol, claveSol, ambiente, greId, greClave }) => {
    const f = new FormData();
    f.append('certificado', archivo);
    f.append('clave_certificado', claveCertificado || '');
    f.append('usuario_sol', usuarioSol);
    f.append('clave_sol', claveSol);
    f.append('ambiente', ambiente);
    // Credenciales API de SUNAT para guías de remisión (opcionales).
    f.append('gre_client_id', greId || '');
    f.append('gre_client_secret', greClave || '');
    return pedir(`/panel/negocios/${id}/facturacion/alta`, { metodo: 'POST', formulario: f });
  },
  cambiarModo: (id, modo) => pedir(`/panel/negocios/${id}/facturacion/modo`, { metodo: 'PUT', cuerpo: { modo } }),
  probar: (id) => pedir(`/panel/negocios/${id}/facturacion/probar`, { metodo: 'POST' }),
};
