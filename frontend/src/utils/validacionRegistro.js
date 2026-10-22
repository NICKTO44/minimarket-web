// Validaciones de FORMATO del formulario de registro.
//
// Es la misma lógica que backend/src/logica/validacion_registro.rs: aquí
// solo sirve para avisar mientras se escribe; el backend vuelve a revisar
// todo al registrar. Si se cambia una regla, hay que cambiarla en los dos.

const PREFIJOS_RUC = ['10', '15', '16', '17', '20'];
const PESOS_RUC = [5, 4, 3, 2, 7, 6, 5, 4, 3, 2];

/** Mensaje si el RUC no tiene un formato posible, o null si está bien. */
export function errorFormatoRuc(ruc) {
  if (!ruc) return 'El RUC es obligatorio.';
  if (!/^\d+$/.test(ruc)) return 'El RUC solo lleva números.';
  if (ruc.length !== 11) return 'El RUC tiene 11 dígitos.';
  if (!PREFIJOS_RUC.includes(ruc.slice(0, 2))) return 'El RUC debe empezar con 10, 15, 16, 17 o 20.';
  const digitos = [...ruc].map(Number);
  const suma = PESOS_RUC.reduce((total, peso, i) => total + digitos[i] * peso, 0);
  let esperado = 11 - (suma % 11);
  if (esperado === 10) esperado = 0;
  if (esperado === 11) esperado = 1;
  if (digitos[10] !== esperado) return 'Ese número de RUC no es válido. Revisa que esté bien escrito.';
  return null;
}

// ---------------------------------------------------------------------
// Texto escrito al azar: solo señales fuertes, palabra por palabra.
// ---------------------------------------------------------------------

const FILAS_TECLADO = ['qwertyuiop', 'asdfghjklñ', 'zxcvbnm'];
const esVocal = (c) => 'aeiouy'.includes(c);

function palabras(texto) {
  // Quita tildes letra por letra, sin perder la ñ.
  const limpio = [...texto.toLowerCase()]
    .map((c) => (c === 'ñ' ? c : c.normalize('NFD').replace(/[\u0300-\u036f]/g, '')))
    .join('');
  return limpio.split(/[^\p{L}]+/u).filter(Boolean).map((p) => [...p]);
}

function palabraAlAzar(p) {
  const largo = p.length;
  if (largo < 4) return false; // siglas e iniciales son normales
  const vocales = p.filter(esVocal).length;

  // 1. Ninguna vocal.
  if (vocales === 0) return true;

  // 2. Cinco consonantes seguidas o más.
  let seguidas = 0;
  for (const c of p) {
    seguidas = esVocal(c) ? 0 : seguidas + 1;
    if (seguidas >= 5) return true;
  }

  // 3. La misma letra tres veces seguidas.
  for (let i = 0; i + 2 < largo; i++) {
    if (p[i] === p[i + 1] && p[i + 1] === p[i + 2]) return true;
  }

  // 4. Un mismo par de letras repetido 3 veces seguidas ("jsjsjs").
  for (let i = 0; i + 5 < largo; i++) {
    if (p[i] !== p[i + 1] && p[i] === p[i + 2] && p[i + 2] === p[i + 4] && p[i + 1] === p[i + 3] && p[i + 3] === p[i + 5]) {
      return true;
    }
  }

  // 5. Casi todo en una fila del teclado y con pocas vocales.
  if (largo >= 5) {
    const enUnaFila = FILAS_TECLADO.some((fila) => p.filter((c) => fila.includes(c)).length / largo >= 0.8);
    if (enUnaFila && vocales / largo < 0.3) return true;
  }

  return false;
}

export function pareceAlAzar(texto) {
  return palabras(texto || '').some(palabraAlAzar);
}

const cantidadLetras = (texto) => (texto.match(/\p{L}/gu) || []).length;

/** Nombre del negocio o de la persona. `campo` va en el mensaje. */
export function errorNombre(texto, campo) {
  const t = (texto || '').trim();
  if (cantidadLetras(t) < 2) return `${campo} debe tener letras.`;
  if (pareceAlAzar(t)) return `${campo} parece escrito al azar. Escríbelo como aparece en tus documentos.`;
  return null;
}

export function errorUsuario(usuario) {
  const u = (usuario || '').trim();
  if (cantidadLetras(u) < 3) return 'El usuario debe tener al menos 3 letras.';
  if (pareceAlAzar(u)) return 'El usuario parece escrito al azar. Usa algo fácil de recordar, como el nombre de tu negocio.';
  return null;
}

// ---------------------------------------------------------------------
// Sugerencias a partir de lo que devuelve la consulta del RUC.
// ---------------------------------------------------------------------

/** Factiliza manda "-" o vacío cuando un dato no figura. */
export function datoReal(texto) {
  const t = (texto || '').trim();
  return /[\p{L}\p{N}]/u.test(t) && t !== '-' ? t : '';
}

const SUFIJOS_SOCIETARIOS = new Set(['sac', 'saa', 'sa', 'eirl', 'srl', 'scrl', 'sociedad', 'anonima', 'cerrada', 'empresa', 'individual', 'responsabilidad', 'limitada']);
const PALABRAS_GENERICAS = new Set(['inversiones', 'corporacion', 'negociaciones', 'comercial', 'comercializadora', 'distribuidora', 'importadora', 'grupo', 'servicios', 'generales', 'multiservicios', 'de', 'del', 'la', 'las', 'los', 'el', 'y', 'e']);

function normalizar(texto) {
  return texto
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/\./g, '') // S.A.C. -> sac
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
}

/**
 * Base para sugerir un usuario. En persona natural la razón social es
 * "APELLIDO APELLIDO NOMBRES": se arma nombre.apellido. En empresa, el
 * nombre comercial o la razón social sin "S.A.C." ni palabras genéricas
 * ("inversiones", "servicios"...), unidas sin espacios.
 * Devuelve '' si no sale nada razonable.
 */
export function baseUsuarioSugerido({ ruc, razonSocial, nombreComercial }) {
  // Une palabras mientras quepan en 20 letras: "ferreteriasanjose".
  const unir = (lista) => lista.reduce((acc, w) => (acc.length + w.length <= 20 ? acc + w : acc), '');
  const sinRelleno = (lista) => {
    const utiles = lista.filter((w) => !SUFIJOS_SOCIETARIOS.has(w));
    const sinGenericas = utiles.filter((w) => !PALABRAS_GENERICAS.has(w));
    return sinGenericas.length ? sinGenericas : utiles;
  };

  let base;
  const comercial = sinRelleno(normalizar(datoReal(nombreComercial)));
  if (comercial.length) {
    base = unir(comercial);
  } else if (ruc?.startsWith('10')) {
    const partes = normalizar(datoReal(razonSocial));
    base = partes.length >= 3 ? `${partes[2]}.${partes[0]}` : partes.join('.');
  } else {
    base = unir(sinRelleno(normalizar(datoReal(razonSocial))));
  }
  base = base.replace(/[^a-z0-9.]/g, '').slice(0, 20).replace(/\.$/, '');
  return base && !errorUsuario(base) ? base : '';
}

/** Variantes a probar, en orden, hasta encontrar una libre. */
export function candidatosUsuario(base, ruc) {
  if (!base) return [];
  const corta = base.slice(0, 16);
  return [base, `${corta}${ruc.slice(-3)}`, `${corta}.pos`, `${corta}${ruc.slice(-4)}`];
}
