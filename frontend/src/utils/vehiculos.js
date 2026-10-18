// Lubricentro: vehículos, kilometraje, órdenes de servicio y paquetes. Las
// reglas de placa y kilometraje deben coincidir con
// backend/src/handlers/vehiculos.rs.
import { hoyLima } from './formato';

/** Placa y kilometraje de la venta, vacíos (panel del punto de venta). */
export const VEHICULO_VACIO = { placa: '', marca: '', modelo: '', anio: '', km: '', proximo_km: '', proxima_fecha: '', encontrado: null };

/** "abc-123" -> "ABC123" (para comparar). */
export function clavePlaca(placa) {
  return String(placa || '')
    .toUpperCase()
    .replace(/[^A-Z0-9]/g, '');
}

/** Como la muestra el servidor: "abc123" -> "ABC-123". null si no parece una placa. */
export function formatoPlaca(placa) {
  const clave = clavePlaca(placa);
  if (clave.length < 4 || clave.length > 8) return null;
  const escrita = String(placa).trim().toUpperCase().split(/\s+/).join('-');
  if (/[^A-Z0-9-]/.test(escrita)) return null;
  return !escrita.includes('-') && clave.length === 6 ? `${clave.slice(0, 3)}-${clave.slice(3)}` : escrita;
}

/** 85000 -> "85,000 km" */
export function textoKm(km) {
  return km == null || km === '' ? '' : `${Number(km).toLocaleString('es-PE')} km`;
}

/** "Toyota Hilux 2015" */
export function nombreVehiculo(v) {
  return [v?.marca, v?.modelo, v?.anio].filter(Boolean).join(' ');
}

/** La fecha de hoy (Perú) más N meses: "2027-01-18". */
export function hoyMasMeses(meses) {
  const [a, m, d] = hoyLima().split('-').map(Number);
  const fecha = new Date(Date.UTC(a, m - 1 + meses, d));
  // 31 de enero + 1 mes no es 3 de marzo: se queda en el último día del mes.
  if (fecha.getUTCDate() !== d) fecha.setUTCDate(0);
  return fecha.toISOString().slice(0, 10);
}

/** Lo que se manda con la venta, o { error } si algo no cuadra. null si no se anotó placa. */
export function vehiculoParaEnviar(datos) {
  if (!datos?.placa?.trim()) return null;
  const placa = formatoPlaca(datos.placa);
  if (!placa) return { error: 'Revisa la placa del vehículo (por ejemplo ABC-123).' };
  const numero = (t) => (String(t ?? '').trim() === '' ? null : Number(String(t).replace(/[,\s]/g, '')));
  const km = numero(datos.km);
  const proximoKm = numero(datos.proximo_km);
  for (const [valor, campo] of [
    [km, 'El kilometraje'],
    [proximoKm, 'El próximo cambio'],
  ]) {
    if (valor !== null && (!Number.isInteger(valor) || valor < 0 || valor > 9999999)) return { error: `${campo} debe ser un número de km.` };
  }
  if (km !== null && proximoKm !== null && proximoKm <= km) {
    return { error: 'El kilometraje del próximo cambio debe ser mayor al de hoy.' };
  }
  const proxima = String(datos.proxima_fecha || '').trim() || null;
  if (proxima && proxima <= hoyLima()) return { error: 'La fecha del próximo servicio debe ser después de hoy.' };
  return {
    vehiculo: {
      placa,
      marca: datos.marca?.trim() || null,
      modelo: datos.modelo?.trim() || null,
      anio: numero(datos.anio),
      km,
      proximo_km: proximoKm,
      proxima_fecha: proxima,
    },
  };
}

/** Enlace de WhatsApp a un número de Perú (9 dígitos) con un mensaje. null si el número no sirve. */
export function enlaceWhatsapp(telefono, mensaje) {
  let numero = String(telefono || '').replace(/\D/g, '');
  if (numero.length === 9) numero = `51${numero}`;
  if (numero.length < 11) return null;
  return `https://wa.me/${numero}?text=${encodeURIComponent(mensaje)}`;
}

/** Mensaje de recordatorio del próximo servicio. */
export function mensajeRecordatorio(vehiculo, nombreNegocio) {
  const quien = vehiculo.cliente_nombre ? `Hola ${vehiculo.cliente_nombre.split(' ')[0]}` : 'Hola';
  const cuando = [
    vehiculo.proxima_fecha ? `el ${vehiculo.proxima_fecha.split('-').reverse().join('/')}` : null,
    vehiculo.proximo_km ? `a los ${textoKm(vehiculo.proximo_km)}` : null,
  ]
    .filter(Boolean)
    .join(' o ');
  return `${quien}, te escribimos de ${nombreNegocio || 'nuestro taller'}. A tu vehículo de placa ${vehiculo.placa}${
    nombreVehiculo(vehiculo) ? ` (${nombreVehiculo(vehiculo)})` : ''
  } le toca su próximo servicio ${cuando || 'pronto'}. ¿Te separamos un horario?`;
}

/**
 * Líneas de un paquete para el carrito: cada producto con su precio de
 * siempre y un descuento repartido en proporción a lo que vale, hasta llegar
 * al precio del paquete (los céntimos que sobran van a la línea más cara).
 * `veces`: cuántos paquetes. Si los productos ya suman menos que el paquete
 * (alguno bajó de precio), van sin descuento.
 */
export function lineasDePaquete(paquete, veces = 1) {
  const lineas = paquete.items.map((i) => {
    const cantidad = Math.round(i.cantidad * veces * 1000) / 1000;
    return { item: i, cantidad, subtotal: Math.round(i.precio * cantidad * 100) / 100 };
  });
  const normal = Math.round(lineas.reduce((s, l) => s + l.subtotal, 0) * 100) / 100;
  const objetivo = Math.round(paquete.precio * veces * 100) / 100;
  const descuento = Math.max(0, Math.round((normal - objetivo) * 100) / 100);
  const repartido = lineas.map((l) => (normal > 0 ? Math.floor((descuento * l.subtotal * 100) / normal) / 100 : 0));
  let resto = Math.round((descuento - repartido.reduce((s, d) => s + d, 0)) * 100);
  const orden = lineas.map((l, i) => i).sort((a, b) => lineas[b].subtotal - lineas[a].subtotal);
  for (let k = 0; resto > 0 && k < orden.length * 2; k++) {
    const i = orden[k % orden.length];
    if (repartido[i] + 0.01 <= lineas[i].subtotal + 1e-9) {
      repartido[i] = Math.round((repartido[i] + 0.01) * 100) / 100;
      resto -= 1;
    }
  }
  return lineas.map((l, i) => ({ producto_id: l.item.producto_id, cantidad: l.cantidad, descuento: repartido[i], subtotal: l.subtotal }));
}
