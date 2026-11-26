import { abreviaturaUnidad } from '../utils/unidades';
import './TicketMesa.css';

// Ticket de la compra por peso (módulo COMPRA_PESO): lo que se le pagó a
// quien trajo el material. Usa el mismo formato angosto que la comanda y la
// precuenta (TicketMesa.css: 80 mm, o 58 mm según utils/papelTicket.js).
// En pantalla no se ve: solo existe para imprimirse con imprimirTicket().

function kilos(n) {
  return Number(n).toLocaleString('es-PE', { maximumFractionDigits: 3 });
}

function soles(n) {
  return Number(n || 0).toFixed(2);
}

export default function TicketCompra({ compra, tienda }) {
  if (!compra) return null;
  return (
    <div className="ticket-mesa">
      <div className="ticket-mesa-centro ticket-mesa-tienda">{tienda?.nombre_tienda || 'Compra'}</div>
      {tienda?.direccion && <div className="ticket-mesa-centro ticket-mesa-meta">{tienda.direccion}</div>}
      {tienda?.telefono && <div className="ticket-mesa-centro ticket-mesa-meta">Cel. {tienda.telefono}</div>}
      <div className="ticket-mesa-linea" />
      <div className="ticket-mesa-centro ticket-mesa-etiqueta">COMPRA</div>
      <div className="ticket-mesa-centro ticket-mesa-meta">
        {compra.folio ? `${compra.folio} · ` : ''}
        {compra.fecha}
      </div>
      {compra.persona && <div className="ticket-mesa-centro ticket-mesa-meta">Vendedor: {compra.persona}</div>}
      <div className="ticket-mesa-linea" />
      {compra.lineas.map((l, i) => (
        <div key={i} className="ticket-mesa-precuenta-item">
          <div className="ticket-mesa-precuenta-fila">
            <span>{l.nombre}</span>
            <span>{soles(l.subtotal)}</span>
          </div>
          <div className="ticket-mesa-opciones">
            {kilos(l.cantidad)} {abreviaturaUnidad(l.unidad)} × S/ {soles(l.precio)}
          </div>
        </div>
      ))}
      <div className="ticket-mesa-linea" />
      <div className="ticket-mesa-precuenta-fila ticket-mesa-total">
        <span>TOTAL PAGADO</span>
        <span>S/ {soles(compra.total)}</span>
      </div>
      <div className="ticket-mesa-linea" />
      <div className="ticket-mesa-centro ticket-mesa-pie">Gracias por su material.</div>
    </div>
  );
}
