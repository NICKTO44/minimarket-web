import { useState, useEffect } from 'react';
import { ArrowDownToLine, ArrowUpFromLine, ReceiptText } from 'lucide-react';
import { api } from '../../api/api';
import { ventasDeCajaPorMetodo } from '../../utils/metodoPago';
import FormularioGasto from '../../components/FormularioGasto';
import MovimientoCaja from '../../components/MovimientoCaja';
import './Caja.css';

const NOMBRE_MOVIMIENTO = { GASTO: 'Gasto', RETIRO: 'Retiro', INGRESO: 'Ingreso' };

export default function Caja({ usuario }) {
  const [caja, setCaja] = useState(null);
  const [cargando, setCargando] = useState(true);
  const [montoInicial, setMontoInicial] = useState('');
  const [montoContado, setMontoContado] = useState('');
  const [observaciones, setObservaciones] = useState('');
  const [procesando, setProcesando] = useState(false);
  const [mensaje, setMensaje] = useState(null);
  const [resumenCierre, setResumenCierre] = useState(null);
  // Gastos, retiros e ingresos del turno, y la ventana abierta para
  // registrar uno ('GASTO' | 'RETIRO' | 'INGRESO').
  const [movimientos, setMovimientos] = useState([]);
  const [ventana, setVentana] = useState(null);

  const cargarCaja = () => {
    setCargando(true);
    api
      .cajaAbierta()
      .then((c) => {
        setCaja(c);
        if (c) api.cajaMovimientos().then(setMovimientos).catch(() => setMovimientos([]));
        else setMovimientos([]);
      })
      .catch(() => setCaja(null))
      .finally(() => setCargando(false));
  };

  const movimientoGuardado = (texto) => {
    setVentana(null);
    setMensaje({ tipo: 'exito', texto });
    cargarCaja();
  };

  useEffect(() => {
    cargarCaja();
  }, []);

  // El efectivo esperado SOLO considera ventas en efectivo — las ventas
  // por tarjeta/transferencia/Yape no afectan el conteo físico de billetes.
  const efectivoEsperado = caja
    ? caja.monto_inicial + caja.ventas_efectivo + caja.ingresos_total - caja.retiros_total - caja.gastos_total
    : 0;

  const diferencia = montoContado !== '' ? parseFloat(montoContado) - efectivoEsperado : null;

  const abrirCaja = async () => {
    setMensaje(null);
    const monto = parseFloat(montoInicial);
    if (isNaN(monto) || monto < 0) {
      setMensaje({ tipo: 'error', texto: 'Ingresa un monto inicial válido.' });
      return;
    }
    setProcesando(true);
    try {
      await api.cajaAbrir({
        usuario_id: usuario.id,
        numero_caja: 1,
        monto_inicial: monto,
        observaciones: observaciones || null,
      });
      setMontoInicial('');
      setObservaciones('');
      setMensaje({ tipo: 'exito', texto: 'Caja abierta correctamente.' });
      cargarCaja();
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setProcesando(false);
    }
  };

  const cerrarCaja = async () => {
    setMensaje(null);
    const monto = parseFloat(montoContado);
    if (isNaN(monto) || monto < 0) {
      setMensaje({ tipo: 'error', texto: 'Ingresa el monto contado en efectivo.' });
      return;
    }
    setProcesando(true);
    try {
      await api.cajaCerrar({
        caja_id: caja.id,
        usuario_id: usuario.id,
        usuario_rol_id: usuario.rol_id,
        monto_contado: monto,
        observaciones: observaciones || null,
      });
      setResumenCierre({
        efectivoEsperado,
        montoContado: monto,
        diferencia: monto - efectivoEsperado,
        // Efectivo, tarjeta, Yape, Plin y transferencia, cada uno en su línea.
        porMetodo: ventasDeCajaPorMetodo(caja),
        totalVentas: caja.total_ventas,
        numeroTransacciones: caja.numero_transacciones,
        devolucionesMonto: caja.devoluciones_monto,
      });
      setMontoContado('');
      setObservaciones('');
      setCaja(null);
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setProcesando(false);
    }
  };

  if (cargando) {
    return (
      <div className="caja-layout">
        <p className="caja-cargando">Cargando estado de caja...</p>
      </div>
    );
  }

  if (resumenCierre) {
    const estadoDif =
      Math.abs(resumenCierre.diferencia) < 0.01
        ? 'sin-diferencia'
        : Math.abs(resumenCierre.diferencia) <= 10
        ? 'aceptable'
        : 'significativa';

    return (
      <div className="caja-layout">
        <div className="caja-card caja-resumen-cierre">
          <h2>Caja cerrada — Reporte del turno</h2>

          <div className="caja-seccion-titulo">Ventas por método de pago</div>
          {resumenCierre.porMetodo.map((m) => (
            <div key={m.clave} className="caja-resumen-fila">
              <span>{m.label}</span>
              <strong>S/ {m.valor.toFixed(2)}</strong>
            </div>
          ))}
                   <div className="caja-resumen-fila caja-resumen-total">
            <span>Total vendido bruto ({resumenCierre.numeroTransacciones} ventas)</span>
            <strong>S/ {resumenCierre.totalVentas.toFixed(2)}</strong>
          </div>
          <div className="caja-resumen-fila">
            <span>Venta neta (descontando devoluciones)</span>
            <strong>S/ {(resumenCierre.totalVentas - resumenCierre.devolucionesMonto).toFixed(2)}</strong>
          </div>
          {resumenCierre.devolucionesMonto > 0 && (
            <div className="caja-resumen-fila caja-fila-negativa">
              <span>Devoluciones</span>
              <strong>- S/ {resumenCierre.devolucionesMonto.toFixed(2)}</strong>
            </div>
          )}

          <div className="caja-separador"></div>

          <div className="caja-seccion-titulo">Cuadre de efectivo</div>
          <div className="caja-resumen-fila">
            <span>Efectivo esperado</span>
            <strong>S/ {resumenCierre.efectivoEsperado.toFixed(2)}</strong>
          </div>
          <div className="caja-resumen-fila">
            <span>Efectivo contado</span>
            <strong>S/ {resumenCierre.montoContado.toFixed(2)}</strong>
          </div>
          <div className={`caja-diferencia caja-diferencia-${estadoDif}`}>
            <span>Diferencia</span>
            <strong>
              {resumenCierre.diferencia >= 0 ? '+' : ''}
              S/ {resumenCierre.diferencia.toFixed(2)}
            </strong>
          </div>

          <button className="caja-boton-primario" onClick={() => setResumenCierre(null)}>
            Abrir nueva caja
          </button>
        </div>
      </div>
    );
  }

  if (!caja) {
    return (
      <div className="caja-layout">
        <div className="caja-card">
          <h2>Abrir caja</h2>
          <p className="caja-subtitulo">No hay ninguna caja abierta en el sistema.</p>

          <label className="caja-label">Monto inicial en efectivo</label>
          <input
            type="number"
            placeholder="0.00"
            value={montoInicial}
            onChange={(e) => setMontoInicial(e.target.value)}
          />

          <label className="caja-label">Observaciones (opcional)</label>
          <textarea
            placeholder="Ej: turno de la mañana"
            value={observaciones}
            onChange={(e) => setObservaciones(e.target.value)}
          />

          {mensaje && <p className={`caja-mensaje caja-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

          <button className="caja-boton-primario" onClick={abrirCaja} disabled={procesando}>
            {procesando ? 'Abriendo...' : 'Abrir caja'}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="caja-layout">
      <div className="caja-card">
        <div className="caja-encabezado">
          <h2>Caja abierta</h2>
          <span className="caja-badge">{caja.usuario_nombre}</span>
        </div>
        <p className="caja-subtitulo">Desde {new Date(caja.fecha_apertura).toLocaleString('es-PE')}</p>

        <div className="caja-seccion-titulo">Ventas por método de pago</div>
        {ventasDeCajaPorMetodo(caja).map((m) => (
          <div key={m.clave} className="caja-resumen-fila">
            <span>{m.label}</span>
            <strong>S/ {m.valor.toFixed(2)}</strong>
          </div>
        ))}
               <div className="caja-resumen-fila caja-resumen-total">
          <span>Total vendido bruto ({caja.numero_transacciones} ventas)</span>
          <strong>S/ {caja.total_ventas.toFixed(2)}</strong>
        </div>
        <div className="caja-resumen-fila">
          <span>Venta neta (descontando devoluciones)</span>
          <strong>S/ {(caja.total_ventas - caja.devoluciones_monto).toFixed(2)}</strong>
        </div>

        <div className="caja-separador"></div>

        <div className="caja-seccion-titulo">Movimientos de efectivo</div>
        <div className="caja-resumen-fila">
          <span>Monto inicial</span>
          <strong>S/ {caja.monto_inicial.toFixed(2)}</strong>
        </div>
        <div className="caja-resumen-fila">
          <span>Ingresos manuales</span>
          <strong>+ S/ {caja.ingresos_total.toFixed(2)}</strong>
        </div>
        <div className="caja-resumen-fila">
          <span>Retiros</span>
          <strong>- S/ {caja.retiros_total.toFixed(2)}</strong>
        </div>
        <div className="caja-resumen-fila">
          <span>Gastos</span>
          <strong>- S/ {caja.gastos_total.toFixed(2)}</strong>
        </div>
        {caja.devoluciones_monto > 0 && (
          <div className="caja-resumen-fila caja-fila-negativa">
            <span>Devoluciones</span>
            <strong>- S/ {caja.devoluciones_monto.toFixed(2)}</strong>
          </div>
        )}
        <div className="caja-resumen-fila caja-resumen-total">
          <span>Efectivo esperado</span>
          <strong>S/ {efectivoEsperado.toFixed(2)}</strong>
        </div>
        {efectivoEsperado < -0.005 && (
          <p className="caja-mov-alerta">
            Se registró salida de más efectivo del que había en la caja. Anula en Gastos el gasto que se pagó con plata de otro lado
            y vuelve a registrarlo con esa forma de pago.
          </p>
        )}

        <div className="caja-mov-acciones">
          <button type="button" onClick={() => setVentana('GASTO')}>
            <ReceiptText size={16} /> Gasto
          </button>
          <button type="button" onClick={() => setVentana('RETIRO')}>
            <ArrowUpFromLine size={16} /> Retiro
          </button>
          <button type="button" onClick={() => setVentana('INGRESO')}>
            <ArrowDownToLine size={16} /> Ingreso
          </button>
        </div>

        {movimientos.length > 0 && (
          <ul className="caja-mov-lista" aria-label="Movimientos del turno">
            {movimientos.map((m) => (
              <li key={m.id}>
                <span className={`caja-mov-tipo caja-mov-${m.tipo.toLowerCase()}`}>{NOMBRE_MOVIMIENTO[m.tipo] || m.tipo}</span>
                <span className="caja-mov-motivo">
                  {m.motivo}
                  <small>
                    {m.hora}
                    {m.usuario ? ` · ${m.usuario}` : ''}
                    {m.gasto_id ? ` · Gasto N.° ${m.gasto_id}` : ''}
                  </small>
                </span>
                <strong>
                  {m.tipo === 'INGRESO' ? '+' : '−'} S/ {m.monto.toFixed(2)}
                </strong>
              </li>
            ))}
          </ul>
        )}

        <div className="caja-separador"></div>

        <h3 className="caja-subtitulo-cierre">Cerrar caja</h3>
        <label className="caja-label">Efectivo contado físicamente</label>
        <input
          type="number"
          placeholder="0.00"
          value={montoContado}
          onChange={(e) => setMontoContado(e.target.value)}
        />

        {diferencia !== null && (
          <p
            className={`caja-diferencia-preview ${
              Math.abs(diferencia) < 0.01 ? 'ok' : Math.abs(diferencia) <= 10 ? 'aceptable' : 'alerta'
            }`}
          >
            Diferencia: {diferencia >= 0 ? '+' : ''}
            S/ {diferencia.toFixed(2)}
          </p>
        )}

        <label className="caja-label">Observaciones (opcional)</label>
        <textarea
          placeholder="Notas sobre el cierre..."
          value={observaciones}
          onChange={(e) => setObservaciones(e.target.value)}
        />

        {mensaje && <p className={`caja-mensaje caja-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

        <button className="caja-boton-cerrar" onClick={cerrarCaja} disabled={procesando}>
          {procesando ? 'Cerrando...' : 'Cerrar caja'}
        </button>
      </div>

      {ventana === 'GASTO' && (
        <FormularioGasto soloCaja efectivoCaja={efectivoEsperado} onCerrar={() => setVentana(null)} onGuardado={(r) => movimientoGuardado(r.mensaje)} />
      )}
      {(ventana === 'RETIRO' || ventana === 'INGRESO') && (
        <MovimientoCaja
          tipo={ventana}
          cajaId={caja.id}
          usuario={usuario}
          disponible={efectivoEsperado}
          onCerrar={() => setVentana(null)}
          onGuardado={() => movimientoGuardado(ventana === 'RETIRO' ? 'Retiro registrado.' : 'Ingreso registrado.')}
        />
      )}
    </div>
  );
}