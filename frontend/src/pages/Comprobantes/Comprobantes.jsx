import { useState, useEffect } from 'react';
import { api } from '../../api/api';
import Recibo from '../../components/Recibo';
import '../../components/Recibo.css';
import './Comprobantes.css';

export default function Comprobantes({ usuario, nombreTienda = 'Mi Minimarket' }) {
  const [comprobantes, setComprobantes] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [filtroTipo, setFiltroTipo] = useState('');
  const [filtroEstado, setFiltroEstado] = useState('');
  const [mensaje, setMensaje] = useState(null);
  const [ventaParaImprimir, setVentaParaImprimir] = useState(null);

  const cargar = () => {
    setCargando(true);
    const filtros = {};
    if (filtroTipo) filtros.tipo = filtroTipo;
    if (filtroEstado) filtros.estado = filtroEstado;
    api
      .comprobantesListar(filtros)
      .then(setComprobantes)
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }))
      .finally(() => setCargando(false));
  };

  useEffect(() => {
    cargar();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filtroTipo, filtroEstado]);

  const reimprimir = async (comp) => {
    setMensaje(null);
    try {
      const detalle = await api.ventaParaDevolucion(comp.folio_venta);
      setVentaParaImprimir({
        venta: { folio: detalle.folio, total: detalle.total, montoRecibido: null, cambio: null },
        items: detalle.productos.map((p) => ({ nombre: p.nombre, cantidad: p.cantidad, precio: p.precio_unitario })),
        comprobante: detalle.comprobante,
        cliente: detalle.cliente || null,
      });
      setTimeout(() => window.print(), 200);
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    }
  };

  return (
    <div className="comp-layout">
      <h1>Comprobantes</h1>
      <p className="comp-subtitulo">Historial de boletas y facturas electrónicas emitidas.</p>

      <div className="comp-filtros">
        <select value={filtroTipo} onChange={(e) => setFiltroTipo(e.target.value)}>
          <option value="">Todos los tipos</option>
          <option value="BOLETA">Boleta</option>
          <option value="FACTURA">Factura</option>
          <option value="NINGUNO">Nota simple</option>
        </select>
        <select value={filtroEstado} onChange={(e) => setFiltroEstado(e.target.value)}>
          <option value="">Todos los estados</option>
          <option value="ACEPTADO">Aceptado</option>
          <option value="RECHAZADO">Rechazado</option>
          <option value="PENDIENTE">Pendiente</option>
        </select>
      </div>

      {mensaje && <p className={`comp-mensaje comp-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

      {cargando ? (
        <p className="comp-cargando">Cargando...</p>
      ) : (
        <div className="comp-tabla-wrapper">
          <table className="comp-tabla">
            <thead>
              <tr>
                <th>Comprobante</th>
                <th>Venta</th>
                <th>Cliente</th>
                <th>Monto</th>
                <th>Estado</th>
                <th>Fecha</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {comprobantes.map((c) => (
                <tr key={c.venta_id}>
                  <td>
                    {c.tipo === 'NINGUNO' ? (
                      <span className="comp-tipo comp-tipo-ninguno">Nota simple</span>
                    ) : (
                      <>
                        <span className="comp-tipo">{c.tipo === 'FACTURA' ? 'Factura' : 'Boleta'}</span>{' '}
                        {c.serie}-{String(c.numero).padStart(6, '0')}
                      </>
                    )}
                  </td>
                  <td className="comp-folio-venta">{c.folio_venta}</td>
                  <td>{c.cliente_nombre || '—'}</td>
                  <td>S/ {c.monto.toFixed(2)}</td>
                  <td>
                    {c.estado ? (
                      <span className={`comp-badge comp-badge-${c.estado.toLowerCase()}`}>{c.estado}</span>
                    ) : (
                      <span className="comp-badge comp-badge-ninguno">Sin comprobante</span>
                    )}
                  </td>
                  <td>{new Date(c.fecha_emision).toLocaleString('es-PE')}</td>
                  <td>
                    <button className="comp-boton-imprimir" onClick={() => reimprimir(c)}>
                      🖨 Imprimir
                    </button>
                  </td>
                </tr>
              ))}
              {comprobantes.length === 0 && (
                <tr>
                  <td colSpan={7} className="comp-sin-resultados">
                    No hay comprobantes que coincidan.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      {ventaParaImprimir && (
        <Recibo
          venta={ventaParaImprimir.venta}
          items={ventaParaImprimir.items}
          comprobante={ventaParaImprimir.comprobante}
          cliente={ventaParaImprimir.cliente}
          nombreTienda={nombreTienda}
          cajero={usuario?.nombre || ''}
        />
      )}
    </div>
  );
}