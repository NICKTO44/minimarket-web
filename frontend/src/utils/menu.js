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
} from 'lucide-react';

export const GRUPOS_MENU = [
  {
    titulo: 'Operación',
    items: [
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

/** Grupos con solo los módulos que ese usuario puede ver. */
export function gruposParaUsuario(usuario) {
  const esAdmin = usuario?.rol_id === 1;
  return GRUPOS_MENU.map((g) => ({ ...g, items: g.items.filter((i) => esAdmin || !i.soloAdmin) })).filter(
    (g) => g.items.length > 0
  );
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
