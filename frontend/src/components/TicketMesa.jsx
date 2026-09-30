import { tituloPedido as titulo } from '../utils/mesas';
import './TicketMesa.css';

// Ticket angosto (80 mm) para la ticketera, en dos versiones:
//  - COMANDA: lo que se manda a preparar a barra/cocina. Sin precios,
//    letra grande, con opciones y notas bien visibles.
//  - PRECUENTA: la cuenta para mostrarle al cliente antes de cobrar. No es
//    un comprobante (la boleta/factura sale al cobrar en el POS).
// En pantalla no se ve: solo existe para imprimirse con window.print().

function horaActual() {
  return new Date().toLocaleString('es-PE', {
    day: '2-digit',
    month: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export default function TicketMesa({ tipo, pedido, items, nombreTienda }) {
  if (!pedido) return null;

  if (tipo === 'COMANDA') {
    return (
      <div className="ticket-mesa">
        <div className="ticket-mesa-centro ticket-mesa-etiqueta">COMANDA</div>
        <div className="ticket-mesa-centro ticket-mesa-destino">{titulo(pedido)}</div>
        <div className="ticket-mesa-centro ticket-mesa-meta">
          Pedido #{pedido.id} · {horaActual()}
          {pedido.mesero ? ` · ${pedido.mesero}` : ''}
        </div>
        <div className="ticket-mesa-linea" />
        {items.map((item) => (
          <div key={item.id} className="ticket-mesa-comanda-item">
            <div className="ticket-mesa-comanda-fila">
              <span className="ticket-mesa-cantidad">{item.cantidad}×</span>
              <span className="ticket-mesa-nombre">{item.nombre_producto}</span>
            </div>
            {item.opciones && <div className="ticket-mesa-opciones">{item.opciones}</div>}
            {item.nota && <div className="ticket-mesa-nota">» {item.nota}</div>}
          </div>
        ))}
        <div className="ticket-mesa-linea" />
      </div>
    );
  }

  const total = items.reduce((s, i) => s + i.subtotal, 0);
  return (
    <div className="ticket-mesa">
      <div className="ticket-mesa-centro ticket-mesa-tienda">{nombreTienda}</div>
      <div className="ticket-mesa-centro ticket-mesa-etiqueta">PRECUENTA</div>
      <div className="ticket-mesa-centro ticket-mesa-meta">
        {titulo(pedido)} · {horaActual()}
      </div>
      <div className="ticket-mesa-linea" />
      {items.map((item) => (
        <div key={item.id} className="ticket-mesa-precuenta-item">
          <div className="ticket-mesa-precuenta-fila">
            <span>
              {item.cantidad} × {item.nombre_producto}
            </span>
            <span>{item.subtotal.toFixed(2)}</span>
          </div>
          {item.opciones && <div className="ticket-mesa-opciones">{item.opciones}</div>}
        </div>
      ))}
      <div className="ticket-mesa-linea" />
      <div className="ticket-mesa-precuenta-fila ticket-mesa-total">
        <span>TOTAL</span>
        <span>S/ {total.toFixed(2)}</span>
      </div>
      <div className="ticket-mesa-linea" />
      <div className="ticket-mesa-centro ticket-mesa-pie">
        Documento no válido como comprobante de pago.
        <br />
        Su boleta o factura se entrega al pagar.
      </div>
    </div>
  );
}
