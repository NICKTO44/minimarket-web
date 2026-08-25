import { useState, useEffect } from 'react';
import {
  Wallet,
  AlertTriangle,
  Clock,
  CheckCircle2,
  ShoppingCart,
  BarChart3,
  TrendingUp,
} from 'lucide-react';
import { api } from '../../api/api';
import './Resumen.css';

const hoy = () => new Date().toISOString().slice(0, 10);

export default function Resumen({ onIrA }) {
  const [caja, setCaja] = useState(null);
  const [estadisticas, setEstadisticas] = useState(null);
  const [stockBajo, setStockBajo] = useState([]);
  const [lotesPorVencer, setLotesPorVencer] = useState([]);
  const [topProductos, setTopProductos] = useState([]);
  const [cargando, setCargando] = useState(true);

  useEffect(() => {
    Promise.all([
      api.cajaAbierta().catch(() => null),
      api.reportesEstadisticas(hoy(), hoy()).catch(() => null),
      api.productosStockBajo().catch(() => []),
      api.lotesPorVencer(7).catch(() => []),
      api.reportesProductosVendidos(hoy(), hoy()).catch(() => []),
    ]).then(([c, e, sb, lv, tp]) => {
      setCaja(c);
      setEstadisticas(e);
      setStockBajo(sb || []);
      setLotesPorVencer(lv || []);
      setTopProductos((tp || []).slice(0, 5));
      setCargando(false);
    });
  }, []);

  if (cargando) {
    return (
      <div className="res-layout">
        <p className="res-cargando">Cargando resumen...</p>
      </div>
    );
  }

  const ventasPorMetodo = caja
    ? [
        { label: 'Efectivo', valor: caja.ventas_efectivo, color: '#16a34a' },
        { label: 'Tarjeta', valor: caja.ventas_tarjeta, color: '#4338ca' },
        { label: 'Transferencia', valor: caja.ventas_transferencia, color: '#0891b2' },
      ]
    : [];
  const maxMetodo = Math.max(1, ...ventasPorMetodo.map((m) => m.valor));
  const maxProducto = Math.max(1, ...topProductos.map((p) => p.cantidad_vendida));

  return (
    <div className="res-layout">
      <h1>Resumen</h1>
      <p className="res-fecha">{new Date().toLocaleDateString('es-PE', { weekday: 'long', day: 'numeric', month: 'long' })}</p>

      <div className={`res-banner-caja ${caja ? 'res-banner-abierta' : 'res-banner-cerrada'}`}>
        {caja ? <Wallet size={18} /> : <AlertTriangle size={18} />}
        {caja ? (
          <span>Caja abierta desde las {new Date(caja.fecha_apertura).toLocaleTimeString('es-PE')} — {caja.usuario_nombre}</span>
        ) : (
          <span>No hay ninguna caja abierta. Ábrela antes de vender.</span>
        )}
      </div>

      {estadisticas && (
        <div className="res-tarjetas">
          <div className="res-tarjeta">
            <span className="res-tarjeta-label">Ventas hoy</span>
            <strong className="res-tarjeta-valor">{estadisticas.ventas_cantidad}</strong>
            <span className="res-tarjeta-sub">S/ {estadisticas.ventas_total.toFixed(2)}</span>
          </div>
          <div className="res-tarjeta">
            <span className="res-tarjeta-label">Ticket promedio</span>
            <strong className="res-tarjeta-valor">S/ {estadisticas.ticket_promedio.toFixed(2)}</strong>
          </div>
          <div className="res-tarjeta res-tarjeta-destacada">
            <span className="res-tarjeta-label">Total neto hoy</span>
            <strong className="res-tarjeta-valor">S/ {estadisticas.total_neto.toFixed(2)}</strong>
          </div>
        </div>
      )}

      <div className="res-columnas">
        {caja && ventasPorMetodo.some((m) => m.valor > 0) && (
          <div className="res-columna">
            <div className="res-columna-header">
              <BarChart3 size={17} className="res-icono-header" />
              <h2>Ventas por método</h2>
            </div>
            <div className="res-chart-barras">
              {ventasPorMetodo.map((m) => (
                <div key={m.label} className="res-barra-fila">
                  <span className="res-barra-label">{m.label}</span>
                  <div className="res-barra-pista">
                    <div
                      className="res-barra-relleno"
                      style={{ width: `${(m.valor / maxMetodo) * 100}%`, background: m.color }}
                    />
                  </div>
                  <span className="res-barra-valor">S/ {m.valor.toFixed(2)}</span>
                </div>
              ))}
            </div>
          </div>
        )}

        {topProductos.length > 0 && (
          <div className="res-columna">
            <div className="res-columna-header">
              <TrendingUp size={17} className="res-icono-header" />
              <h2>Top productos hoy</h2>
            </div>
            <div className="res-chart-barras">
              {topProductos.map((p, idx) => (
                <div key={idx} className="res-barra-fila">
                  <span className="res-barra-label res-barra-label-producto">{p.producto_nombre}</span>
                  <div className="res-barra-pista">
                    <div
                      className="res-barra-relleno"
                      style={{ width: `${(p.cantidad_vendida / maxProducto) * 100}%`, background: '#f59e0b' }}
                    />
                  </div>
                  <span className="res-barra-valor">{p.cantidad_vendida}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      <div className="res-columnas">
        <div className="res-columna">
          <div className="res-columna-header">
            <AlertTriangle size={17} className="res-icono-header res-icono-alerta" />
            <h2>Stock bajo</h2>
            {stockBajo.length > 0 && <span className="res-contador">{stockBajo.length}</span>}
          </div>
          {stockBajo.length === 0 ? (
            <p className="res-vacio">
              <CheckCircle2 size={15} /> Todo el inventario está en niveles normales.
            </p>
          ) : (
            <div className="res-lista">
              {stockBajo.slice(0, 6).map((p) => (
                <div key={p.id} className="res-lista-item">
                  <span>{p.nombre}</span>
                  <strong className="res-stock-critico">{p.stock} {p.unidad_medida}</strong>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="res-columna">
          <div className="res-columna-header">
            <Clock size={17} className="res-icono-header res-icono-alerta" />
            <h2>Vencen pronto</h2>
            {lotesPorVencer.length > 0 && <span className="res-contador">{lotesPorVencer.length}</span>}
          </div>
          {lotesPorVencer.length === 0 ? (
            <p className="res-vacio">
              <CheckCircle2 size={15} /> Nada vence en los próximos 7 días.
            </p>
          ) : (
            <div className="res-lista">
              {lotesPorVencer.slice(0, 6).map((l) => (
                <div key={l.lote_id} className="res-lista-item">
                  <span>{l.producto_nombre}</span>
                  <strong className={l.dias_restantes < 0 ? 'res-stock-critico' : 'res-dias-restantes'}>
                    {l.dias_restantes < 0 ? 'Vencido' : `${l.dias_restantes}d`}
                  </strong>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      <div className="res-accesos">
        <button onClick={() => onIrA && onIrA('POS')}>
          <ShoppingCart size={16} /> Ir a Punto de Venta
        </button>
        <button onClick={() => onIrA && onIrA('CAJA')}>
          <Wallet size={16} /> Ir a Caja
        </button>
        <button onClick={() => onIrA && onIrA('REPORTES')}>
          <BarChart3 size={16} /> Ver Reportes
        </button>
      </div>
    </div>
  );
}