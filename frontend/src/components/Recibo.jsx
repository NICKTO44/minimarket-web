export default function Recibo({ venta, items, nombreTienda, direccion, telefono, cajero, comprobante, cliente }) {
  const encabezado = comprobante
    ? `${comprobante.tipo === 'FACTURA' ? 'FACTURA ELECTRÓNICA' : 'BOLETA ELECTRÓNICA'} ${comprobante.serie}-${String(comprobante.numero).padStart(6, '0')}`
    : 'NOTA DE VENTA (sin comprobante tributario)';

  const totalUnidades = items.reduce((sum, item) => sum + item.cantidad, 0);

  return (
    <div className="recibo-imprimible">
      <div className="recibo-centro recibo-nombre-tienda">{nombreTienda}</div>
      {direccion && <div className="recibo-centro recibo-dato-tienda">{direccion}</div>}
      {telefono && <div className="recibo-centro recibo-dato-tienda">Tel: {telefono}</div>}

      <div className="recibo-linea"></div>
      <div className="recibo-centro recibo-comprobante-tipo">{encabezado}</div>
      <div className="recibo-linea"></div>

      <div className="recibo-fila-meta">
        <span>Venta</span>
        <span>{venta.folio}</span>
      </div>
      <div className="recibo-fila-meta">
        <span>Fecha</span>
        <span>{new Date().toLocaleString('es-PE')}</span>
      </div>
      <div className="recibo-fila-meta">
        <span>Cajero</span>
        <span>{cajero}</span>
      </div>
      {cliente && (
        <div className="recibo-fila-meta">
          <span>Cliente</span>
          <span>
            {cliente.nombre_razon_social}
            {cliente.numero_documento ? ` (${cliente.numero_documento})` : ''}
          </span>
        </div>
      )}

      <div className="recibo-linea"></div>

      {items.map((item, idx) => (
        <div key={idx} className="recibo-item">
          <div className="recibo-item-nombre">{item.nombre}</div>
          <div className="recibo-item-detalle">
            <span>{item.cantidad} x S/.{item.precio.toFixed(2)}</span>
            <span>S/.{(item.precio * item.cantidad).toFixed(2)}</span>
          </div>
        </div>
      ))}

      <div className="recibo-linea"></div>

      <div className="recibo-fila-meta recibo-fila-meta-sutil">
        <span>Ítems</span>
        <span>{items.length} producto{items.length === 1 ? '' : 's'} · {totalUnidades} unid.</span>
      </div>

      <div className="recibo-linea-doble"></div>

      <div className="recibo-total">
        <span>TOTAL</span>
        <span>S/.{venta.total.toFixed(2)}</span>
      </div>

      <div className="recibo-linea-doble"></div>

      {venta.montoRecibido != null && (
        <div className="recibo-detalle-pago">
          <span>Efectivo</span>
          <span>S/.{venta.montoRecibido.toFixed(2)}</span>
        </div>
      )}
      {venta.cambio != null && (
        <div className="recibo-detalle-pago">
          <span>Cambio</span>
          <span>S/.{venta.cambio.toFixed(2)}</span>
        </div>
      )}

      <div className="recibo-linea"></div>
      <div className="recibo-centro recibo-gracias">¡Gracias por su compra!</div>
    </div>
  );
}