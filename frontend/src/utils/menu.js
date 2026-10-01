// Módulos del sistema, compartidos por el Sidebar (computadora) y la
// NavegacionMovil (celular), para que ambos muestren exactamente lo
// mismo. soloAdmin: el cajero no lo ve (el backend además lo bloquea).
import {
  ScanBarcode,
  LayoutGrid,
  Wallet,
  History,
  Package,
  Boxes,
  Truck,
  Undo2,
  Receipt,
  BarChart3,
  Settings,
  Users,
  CreditCard,
  UtensilsCrossed,
  ChefHat,
  ClipboardList,
} from 'lucide-react';

export const GRUPOS_MENU = [
  {
    titulo: 'Operación',
    items: [
      { id: 'MESAS', label: 'Mesas', icono: UtensilsCrossed, soloRestaurante: true },
      { id: 'PREPARACION', label: 'Preparación', icono: ChefHat, soloRestaurante: true },
      { id: 'CARTA', label: 'Carta de hoy', icono: ClipboardList, soloRestaurante: true, sinCajero: true },
      { id: 'POS', label: 'Punto de Venta', icono: ScanBarcode },
      { id: 'RESUMEN', label: 'Resumen', icono: LayoutGrid },
      { id: 'CAJA', label: 'Caja y Turnos', icono: Wallet },
      { id: 'HISTORIAL_CAJA', label: 'Historial de Caja', icono: History },
      { id: 'CLIENTES', label: 'Clientes', icono: Users },
    ],
  },
  {
    titulo: 'Inventario',
    items: [
      { id: 'PRODUCTOS', label: 'Productos', icono: Package },
      { id: 'STOCK', label: 'Stock y Lotes', icono: Boxes },
      { id: 'PROVEEDORES', label: 'Proveedores', icono: Truck },
      { id: 'DEVOLUCIONES', label: 'Devoluciones', icono: Undo2 },
    ],
  },
  {
    titulo: 'Administración',
    items: [
      { id: 'COMPROBANTES', label: 'Comprobantes', icono: Receipt },
      { id: 'REPORTES', label: 'Reportes', icono: BarChart3 },
      { id: 'SUSCRIPCION', label: 'Suscripción', icono: CreditCard, soloAdmin: true },
      { id: 'CONFIGURACION', label: 'Configuración', icono: Settings, soloAdmin: true },
    ],
  },
];

/** true si el usuario es Mesero (su id de rol puede variar entre negocios). */
export function esMesero(usuario) {
  return usuario?.rol_nombre === 'MESERO';
}

/** true si el usuario es de barra/cocina: solo marca pedidos listos. */
export function esPreparacion(usuario) {
  return usuario?.rol_nombre === 'PREPARACION';
}

/** true si es Cajero (solo cobra; no arma la carta del día). */
export function esCajero(usuario) {
  return usuario?.rol_nombre === 'CAJERO';
}

/** Pantallas que puede abrir cada rol limitado (modo restaurante). */
export const PANTALLAS_MESERO = ['MESAS', 'CARTA'];
export const PANTALLAS_PREPARACION = ['PREPARACION', 'CARTA'];

/** Nombre del rol para mostrar en el menú. */
export function etiquetaRol(usuario) {
  if (usuario?.rol_id === 1) return 'Administrador';
  if (esMesero(usuario)) return 'Mesero';
  if (esPreparacion(usuario)) return 'Barra / Cocina';
  return 'Cajero';
}

/**
 * Grupos con solo los módulos que ese usuario puede ver.
 * restaurante: el negocio atiende en mesas (muestra "Mesas").
 * El mesero solo ve Mesas y la Carta de hoy: toma pedidos, no cobra.
 * Barra/Cocina solo ve Preparación y la Carta de hoy (marca agotados).
 */
export function gruposParaUsuario(usuario, { restaurante = false } = {}) {
  const esAdmin = usuario?.rol_id === 1;
  const mesero = restaurante && esMesero(usuario);
  const preparacion = restaurante && esPreparacion(usuario);
  return GRUPOS_MENU.map((g) => ({
    ...g,
    items: g.items.filter((i) => {
      if (i.soloRestaurante && !restaurante) return false;
      if (mesero) return PANTALLAS_MESERO.includes(i.id);
      if (preparacion) return PANTALLAS_PREPARACION.includes(i.id);
      // El cajero solo cobra: la carta del día la arman los demás.
      if (i.sinCajero && esCajero(usuario)) return false;
      return esAdmin || !i.soloAdmin;
    }),
  })).filter((g) => g.items.length > 0);
}

/**
 * Nivel del aviso de suscripción: solo cuando de verdad importa (pocos
 * días o ya vencida). 'vencido' | 'critico' | 'alerta' | null.
 */
export function nivelAvisoSuscripcion(diasRestantes) {
  if (diasRestantes == null) return null;
  if (diasRestantes < 0) return 'vencido';
  if (diasRestantes <= 7) return 'critico';
  if (diasRestantes <= 15) return 'alerta';
  return null;
}
