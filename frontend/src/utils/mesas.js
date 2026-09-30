// Textos compartidos del módulo Cafetería / Restaurante.

/** "recién", "25 min", "1 h 10 min" */
export function tiempoAbierto(minutos) {
  if (minutos == null) return '';
  if (minutos < 1) return 'recién';
  if (minutos < 60) return `${minutos} min`;
  const h = Math.floor(minutos / 60);
  const m = minutos % 60;
  return m ? `${h} h ${m} min` : `${h} h`;
}

/** "Mesa 3", "Para llevar · Ana", "Delivery" */
export function tituloPedido(p) {
  if (!p) return '';
  if (p.tipo === 'MESA') return p.mesa_nombre || 'Mesa';
  const base = p.tipo === 'DELIVERY' ? 'Delivery' : 'Para llevar';
  return p.cliente_nombre ? `${base} · ${p.cliente_nombre}` : base;
}
