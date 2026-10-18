import { useState } from 'react';
import { api } from '../api/api';
import { VEHICULO_VACIO, clavePlaca, formatoPlaca, hoyMasMeses, nombreVehiculo, textoKm } from '../utils/vehiculos';
import './VehiculoVenta.css';

/**
 * Placa, kilometraje y próximo servicio de la venta (módulo "Vehículos y
 * kilometraje"). Todo es opcional: sin placa, la venta es la de siempre.
 * Al escribir una placa conocida trae el vehículo y su último servicio; si
 * el vehículo tiene dueño y la venta aún no tiene cliente, lo sugiere.
 * `fija`: la placa viene de una orden de servicio y no se cambia aquí.
 */
export default function VehiculoVenta({ datos, onCambiar, onDueno, fija = false }) {
  const [abierto, setAbierto] = useState(!!datos.placa);
  // Con la placa ya anotada se puede plegar en una línea, para que el
  // carrito se vea entero (sobre todo en el celular).
  const [plegado, setPlegado] = useState(fija && !!datos.placa);
  const [buscando, setBuscando] = useState(false);
  const [aviso, setAviso] = useState(null);
  // Siempre sobre lo último escrito: la búsqueda de la placa responde
  // después, cuando ya pudieron escribir el kilometraje.
  const cambiar = (campo, valor) => onCambiar((d) => ({ ...d, [campo]: valor }));

  const buscar = async () => {
    const placa = formatoPlaca(datos.placa);
    if (!placa) {
      setAviso(datos.placa.trim() ? 'Revisa la placa (por ejemplo ABC-123).' : null);
      return;
    }
    if (datos.encontrado && clavePlaca(datos.encontrado.placa) === clavePlaca(placa)) return;
    setBuscando(true);
    setAviso(null);
    try {
      const v = await api.vehiculoPorPlaca(placa);
      onCambiar((d) => ({
        ...d,
        placa: v.placa,
        marca: v.marca || '',
        modelo: v.modelo || '',
        anio: v.anio || '',
        encontrado: v,
      }));
      if (v.cliente_id) {
        onDueno?.({
          id: v.cliente_id,
          nombre_razon_social: v.cliente_nombre,
          numero_documento: v.cliente_documento || null,
          tipo_documento: v.cliente_tipo_documento || null,
          telefono: v.cliente_telefono || null,
        });
      }
    } catch {
      // No existe: es un vehículo nuevo (se crea con la venta).
      onCambiar((d) => ({ ...d, placa, encontrado: null }));
      setAviso('Vehículo nuevo: se registra con esta venta.');
    } finally {
      setBuscando(false);
    }
  };

  const kmNumero = Number(String(datos.km || '').replace(/[,\s]/g, '')) || 0;

  if (!abierto) {
    return (
      <button type="button" className="vv-abrir" onClick={() => setAbierto(true)}>
        🚗 Placa y kilometraje (opcional)
      </button>
    );
  }

  if (plegado && datos.placa) {
    const proximo = [
      datos.proximo_km ? textoKm(datos.proximo_km) : null,
      datos.proxima_fecha ? datos.proxima_fecha.split('-').reverse().join('/') : null,
    ]
      .filter(Boolean)
      .join(' o ');
    return (
      <button type="button" className="vv-abrir vv-resumen" onClick={() => setPlegado(false)} title="Cambiar placa o kilometraje">
        🚗 <strong>{datos.placa}</strong>
        {datos.km ? ` · ${textoKm(datos.km)}` : ''}
        {proximo ? ` · próx. ${proximo}` : ''} ✎
      </button>
    );
  }

  const e = datos.encontrado;
  return (
    <div className="vv-caja">
      <div className="vv-fila">
        <input
          className="vv-placa"
          value={datos.placa}
          placeholder="Placa"
          aria-label="Placa del vehículo"
          maxLength={10}
          disabled={fija}
          onChange={(ev) => cambiar('placa', ev.target.value.toUpperCase())}
          onBlur={buscar}
          onKeyDown={(ev) => ev.key === 'Enter' && (ev.preventDefault(), buscar())}
        />
        <input
          className="vv-km"
          inputMode="numeric"
          value={datos.km}
          placeholder="Km de hoy"
          aria-label="Kilometraje de hoy"
          onChange={(ev) => cambiar('km', ev.target.value.replace(/[^\d]/g, ''))}
        />
        {datos.placa && (
          <button type="button" className="vv-listo" onClick={() => setPlegado(true)}>
            Listo
          </button>
        )}
        {!fija && (
          <button
            type="button"
            className="vv-quitar"
            aria-label="Quitar la placa"
            onClick={() => {
              onCambiar(() => ({ ...VEHICULO_VACIO }));
              setAviso(null);
              setAbierto(false);
            }}
          >
            ×
          </button>
        )}
      </div>
      {buscando && <p className="vv-nota">Buscando la placa…</p>}
      {e && (
        <p className="vv-nota">
          {nombreVehiculo(e) || 'Vehículo registrado'}
          {e.km_ultimo != null && ` · último: ${textoKm(e.km_ultimo)}`}
          {e.ultimo_servicio && ` el ${e.ultimo_servicio.slice(0, 10).split('-').reverse().join('/')}`}
        </p>
      )}
      {!e && datos.placa && !buscando && (
        <div className="vv-fila">
          <input
            value={datos.marca}
            placeholder="Marca"
            aria-label="Marca"
            maxLength={40}
            onChange={(ev) => cambiar('marca', ev.target.value)}
          />
          <input
            value={datos.modelo}
            placeholder="Modelo"
            aria-label="Modelo"
            maxLength={60}
            onChange={(ev) => cambiar('modelo', ev.target.value)}
          />
          <input
            className="vv-anio"
            inputMode="numeric"
            value={datos.anio}
            placeholder="Año"
            aria-label="Año"
            onChange={(ev) => cambiar('anio', ev.target.value.replace(/[^\d]/g, '').slice(0, 4))}
          />
        </div>
      )}
      {aviso && <p className="vv-nota">{aviso}</p>}
      <p className="vv-titulo">Próximo servicio (sale en el ticket)</p>
      <div className="vv-fila">
        <input
          className="vv-km"
          inputMode="numeric"
          value={datos.proximo_km}
          placeholder="Km"
          aria-label="Kilometraje del próximo servicio"
          onChange={(ev) => cambiar('proximo_km', ev.target.value.replace(/[^\d]/g, ''))}
        />
        <input
          type="date"
          value={datos.proxima_fecha}
          aria-label="Fecha del próximo servicio"
          onChange={(ev) => cambiar('proxima_fecha', ev.target.value)}
        />
      </div>
      {/* Atajos solo mientras falta el dato: el panel ocupa menos en el carrito. */}
      {((kmNumero > 0 && !datos.proximo_km) || !datos.proxima_fecha) && (
        <div className="vv-atajos">
          {kmNumero > 0 &&
            !datos.proximo_km &&
            [3000, 5000, 10000].map((n) => (
              <button key={n} type="button" onClick={() => cambiar('proximo_km', String(kmNumero + n))}>
                +{n.toLocaleString('es-PE')} km
              </button>
            ))}
          {!datos.proxima_fecha &&
            [3, 6].map((m) => (
              <button key={m} type="button" onClick={() => cambiar('proxima_fecha', hoyMasMeses(m))}>
                {m} meses
              </button>
            ))}
        </div>
      )}
    </div>
  );
}
