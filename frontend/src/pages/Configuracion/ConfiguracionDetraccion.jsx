import { useEffect, useState } from 'react';
import { api } from '../../api/api';
import './ConfiguracionDetraccion.css';

// Códigos frecuentes del catálogo 54 de SUNAT. El porcentaje de cada uno lo
// fija la norma y puede cambiar: por eso se escribe aparte y es editable.
const CODIGOS = [
  { valor: '008', label: '008 · Madera' },
  { valor: '009', label: '009 · Arena y piedra' },
  { valor: '019', label: '019 · Arrendamiento de bienes muebles' },
  { valor: '020', label: '020 · Mantenimiento y reparación de bienes muebles' },
  { valor: '022', label: '022 · Otros servicios empresariales' },
  { valor: '030', label: '030 · Contratos de construcción' },
  { valor: '037', label: '037 · Demás servicios gravados con el IGV' },
];
const OTRO = 'OTRO';

/**
 * Configuración → Detracción (módulo "Detracción en facturas"). El sistema
 * no decide el impuesto: aplica el porcentaje, el código y el monto mínimo
 * que el administrador escriba aquí, en las facturas que superen ese monto.
 * Sin la cuenta del Banco de la Nación todavía no se aplica.
 */
export default function ConfiguracionDetraccion({ onMensaje }) {
  const [form, setForm] = useState(null);
  const [guardada, setGuardada] = useState(null);
  const [otroCodigo, setOtroCodigo] = useState(false);
  const [guardando, setGuardando] = useState(false);

  const cargar = (d) => {
    setGuardada(d);
    setForm({ porcentaje: String(d.porcentaje), codigo: d.codigo, minimo: String(d.minimo), cuenta: d.cuenta || '' });
    setOtroCodigo(!CODIGOS.some((c) => c.valor === d.codigo));
  };

  useEffect(() => {
    api
      .detraccion()
      .then(cargar)
      .catch((e) => onMensaje({ tipo: 'error', texto: e.message }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!form) return null;

  const porcentaje = Number(String(form.porcentaje).replace(',', '.'));
  const minimo = Number(String(form.minimo).replace(',', '.'));
  const porcentajeOk = porcentaje > 0 && porcentaje <= 30;
  const codigoOk = /^\d{3}$/.test(form.codigo.trim());
  const minimoOk = Number.isFinite(minimo) && minimo >= 0;
  const cuenta = form.cuenta.trim();
  const cuentaOk = cuenta === '' || (/^[\d\- ]+$/.test(cuenta) && cuenta.replace(/\D/g, '').length >= 6);
  const sinCambios =
    guardada &&
    porcentaje === guardada.porcentaje &&
    form.codigo.trim() === guardada.codigo &&
    minimo === guardada.minimo &&
    cuenta === (guardada.cuenta || '');
  const puedeGuardar = porcentajeOk && codigoOk && minimoOk && cuentaOk && !sinCambios && !guardando;

  const guardar = async () => {
    setGuardando(true);
    onMensaje(null);
    try {
      const d = await api.detraccionGuardar({ porcentaje, codigo: form.codigo.trim(), minimo, cuenta });
      cargar(d);
      onMensaje({
        tipo: 'exito',
        texto: d.lista
          ? `Detracción guardada: ${d.porcentaje} % en facturas mayores a S/ ${d.minimo.toFixed(2)}.`
          : 'Detracción guardada. Falta la cuenta del Banco de la Nación para que se aplique.',
      });
    } catch (e) {
      onMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardando(false);
    }
  };

  // Ejemplo con una factura de S/ 1,180 (o el doble del mínimo si es mayor).
  const ejemplo = Math.max(1180, minimoOk ? minimo * 2 : 0);
  const montoEjemplo = porcentajeOk ? Math.round(ejemplo * porcentaje) / 100 : 0;

  return (
    <div className="cfg-card det-card">
      <div className="det-cabecera">
        <h3 className="cfg-subtitulo-seccion">Detracción en facturas</h3>
        <span className={`det-estado${guardada.lista ? ' det-estado-lista' : ''}`}>
          {guardada.lista ? 'Activa' : 'Falta la cuenta'}
        </span>
      </div>
      <p className="cfg-nota-moneda">
        Cuando una <strong>factura</strong> supera el monto mínimo, sale como operación sujeta a detracción: el cliente
        deposita el porcentaje en tu cuenta del Banco de la Nación y te paga el resto. Las boletas no la llevan. Si la
        norma cambia el porcentaje, lo cambias aquí y el sistema usa el nuevo.
      </p>

      <div className="cfg-campo-fila">
        <div className="cfg-campo">
          <label>Porcentaje (%)</label>
          <input
            inputMode="decimal"
            value={form.porcentaje}
            onChange={(e) => setForm({ ...form, porcentaje: e.target.value })}
            aria-invalid={!porcentajeOk}
          />
          {!porcentajeOk && <p className="det-error">Debe ser mayor a 0 y hasta 30.</p>}
        </div>
        <div className="cfg-campo">
          <label>Se aplica si el total supera (S/)</label>
          <input
            inputMode="decimal"
            value={form.minimo}
            onChange={(e) => setForm({ ...form, minimo: e.target.value })}
            aria-invalid={!minimoOk}
          />
        </div>
      </div>

      <div className="cfg-campo">
        <label>Bien o servicio (código SUNAT)</label>
        <select
          value={otroCodigo ? OTRO : form.codigo}
          onChange={(e) => {
            if (e.target.value === OTRO) {
              setOtroCodigo(true);
              setForm({ ...form, codigo: '' });
            } else {
              setOtroCodigo(false);
              setForm({ ...form, codigo: e.target.value });
            }
          }}
        >
          {CODIGOS.map((c) => (
            <option key={c.valor} value={c.valor}>
              {c.label}
            </option>
          ))}
          <option value={OTRO}>Otro código…</option>
        </select>
        {otroCodigo && (
          <input
            className="det-otro-codigo"
            inputMode="numeric"
            maxLength={3}
            placeholder="Tres dígitos, por ejemplo 014"
            value={form.codigo}
            onChange={(e) => setForm({ ...form, codigo: e.target.value.replace(/\D/g, '') })}
            aria-label="Otro código de detracción"
          />
        )}
      </div>

      <div className="cfg-campo">
        <label>Cuenta de detracciones (Banco de la Nación)</label>
        <input
          inputMode="numeric"
          placeholder="00-000-000000"
          value={form.cuenta}
          onChange={(e) => setForm({ ...form, cuenta: e.target.value })}
          aria-invalid={!cuentaOk}
        />
        {!cuentaOk && <p className="det-error">Solo números y guiones.</p>}
        {cuentaOk && cuenta === '' && (
          <p className="det-aviso">Sin la cuenta, las facturas siguen saliendo normales (sin detracción).</p>
        )}
      </div>

      {porcentajeOk && (
        <p className="det-ejemplo">
          Ejemplo: factura de S/ {ejemplo.toFixed(2)} → detracción S/ {montoEjemplo.toFixed(2)}; el cliente te paga S/{' '}
          {(ejemplo - montoEjemplo).toFixed(2)}.
        </p>
      )}

      <p className="cfg-nota-moneda">
        El sistema aplica lo que escribas aquí; no decide el impuesto. Confirma el porcentaje y el código con tu
        contador.
      </p>

      <button className="cfg-boton-guardar" onClick={guardar} disabled={!puedeGuardar}>
        {guardando ? 'Guardando...' : 'Guardar detracción'}
      </button>
    </div>
  );
}
