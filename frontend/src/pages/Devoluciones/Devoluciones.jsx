import { useState } from 'react';
import { api } from '../../api/api';
import './Devoluciones.css';

export default function Devoluciones({ usuario }) {
  const [busqueda, setBusqueda] = useState('');
  const [buscando, setBuscando] = useState(false);
  const [venta, setVenta] = useState(null);
  const [items, setItems] = useState([]);
  const [motivo, setMotivo] = useState('');
  const [procesando, setProcesando] = useState(false);
  const [mensaje, setMensaje] = useState(null);

  const buscar = async () => {
    if (!busqueda.trim()) return;
    setMensaje(null);
    setVenta(null);
    setBuscando(true);
    try {
      const resultado = await api.ventaParaDevolucion(busqueda.trim());
      setVenta(resultado);
      setItems(
        resultado.productos.map((p) => ({
          detalle_id: p.detalle_id,
          producto_id: p.producto_id,
          nombre: p.nombre,
          cantidad_original: p.cantidad,
          precio_unitario: p.precio_unitario,
          cantidad_a_devolver: '0',
          condicion: 'REVENTA',
        }))
      );
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setBuscando(false);
    }
  };

  const manejarEnter = (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      buscar();
    }
  };

  const cambiarItem = (idx, campo, valor) => {
    setItems((prev) => prev.map((it, i) => (i === idx ? { ...it, [campo]: valor } : it)));
  };

  const totalADevolver = items.reduce(
    (sum, it) => sum + (parseFloat(it.cantidad_a_devolver) || 0) * it.precio_unitario,
    0
  );

  const procesarDevolucion = async () => {
    setMensaje(null);
    if (!motivo.trim()) {
      setMensaje({ tipo: 'error', texto: 'Indica el motivo de la devolución.' });
      return;
    }

    const productosADevolver = items
      .filter((it) => parseFloat(it.cantidad_a_devolver) > 0)
      .map((it) => ({
        detalle_id: it.detalle_id,
        producto_id: it.producto_id,
        cantidad: parseFloat(it.cantidad_a_devolver),
      }));

    if (productosADevolver.length === 0) {
      setMensaje({ tipo: 'error', texto: 'Indica cuántas unidades devuelve el cliente de al menos un producto.' });
      return;
    }

    for (const it of items) {
      const cant = parseFloat(it.cantidad_a_devolver) || 0;
      if (cant > it.cantidad_original) {
        setMensaje({ tipo: 'error', texto: `No puedes devolver más de lo vendido de "${it.nombre}".` });
        return;
      }
    }

    setProcesando(true);
    try {
      const resultado = await api.devolucionCrear({
        venta_id: venta.venta_id,
        productos: productosADevolver,
        motivo: motivo.trim(),
        usuario_id: usuario.id,
      });
      setMensaje({ tipo: 'exito', texto: `Devolución ${resultado.folio_devolucion || ''} procesada correctamente. Stock actualizado.` });
      setVenta(null);
      setItems([]);
      setBusqueda('');
      setMotivo('');
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setProcesando(false);
    }
  };

  return (
    <div className="dev-layout">
      <h1>Devoluciones</h1>
      <p className="dev-subtitulo">
        Busca por el folio interno de la venta (ej: V-20260822-0001) o por el número de boleta/factura
        (ej: B001-000004) — el que traiga el cliente en su ticket.
      </p>

      <div className="dev-buscador">
        <input
          type="text"
          placeholder="Folio de venta o número de comprobante..."
          value={busqueda}
          onChange={(e) => setBusqueda(e.target.value)}
          onKeyDown={manejarEnter}
        />
        <button onClick={buscar} disabled={buscando}>
          {buscando ? 'Buscando...' : 'Buscar'}
        </button>
      </div>

      {mensaje && <p className={`dev-mensaje dev-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

      {venta && (
        <div className="dev-venta-card">
          <div className="dev-venta-header">
            <div>
              <span className="dev-venta-folio">{venta.folio}</span>
              {venta.comprobante && (
                <span className="dev-venta-comprobante">
                  {venta.comprobante.tipo === 'FACTURA' ? 'Factura' : 'Boleta'} {venta.comprobante.serie}-
                  {String(venta.comprobante.numero).padStart(6, '0')}
                </span>
              )}
            </div>
            <span className="dev-venta-fecha">{new Date(venta.fecha_hora).toLocaleString('es-PE')}</span>
          </div>
          <div className="dev-venta-total">Total original: S/ {venta.total.toFixed(2)} — {venta.metodo_pago}</div>

          <div className="dev-items">
            {items.map((it, idx) => (
              <div key={it.detalle_id} className="dev-item">
                <div className="dev-item-info">
                  <span className="dev-item-nombre">{it.nombre}</span>
                  <span className="dev-item-detalle">
                    Vendidos: {it.cantidad_original} × S/ {it.precio_unitario.toFixed(2)}
                  </span>
                </div>
                <div className="dev-item-controles">
                  <input
                    type="number"
                    min="0"
                    max={it.cantidad_original}
                    placeholder="Cant."
                    value={it.cantidad_a_devolver}
                    onChange={(e) => cambiarItem(idx, 'cantidad_a_devolver', e.target.value)}
                  />
                  <select value={it.condicion} onChange={(e) => cambiarItem(idx, 'condicion', e.target.value)}>
                    <option value="REVENTA">Buen estado (vuelve al stock)</option>
                    <option value="DEFECTUOSO">Defectuoso</option>
                    <option value="VENCIDO">Vencido</option>
                  </select>
                </div>
              </div>
            ))}
          </div>

          <div className="dev-campo">
            <label>Motivo de la devolución</label>
            <input value={motivo} onChange={(e) => setMotivo(e.target.value)} placeholder="Ej: cliente se arrepintió, producto en mal estado..." />
          </div>

          <div className="dev-total-row">
            <span>Total a devolver</span>
            <strong>S/ {totalADevolver.toFixed(2)}</strong>
          </div>

          <button className="dev-boton-procesar" onClick={procesarDevolucion} disabled={procesando}>
            {procesando ? 'Procesando...' : 'Procesar devolución'}
          </button>
        </div>
      )}
    </div>
  );
}