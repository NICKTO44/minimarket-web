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
  FileText,
  HandCoins,
  Route,
  TrendingDown,
  FileCheck2,
  Car,
  Wrench,
  PackageOpen,
  CakeSlice,
  Wheat,
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
      // Solo con su módulo encendido (ver utils/rubros.js).
      { id: 'COTIZACIONES', label: 'Cotizaciones', icono: FileText, modulo: 'COTIZACIONES' },
      { id: 'CREDITOS', label: 'Créditos', icono: HandCoins, modulo: 'CREDITO' },
      { id: 'GUIAS', label: 'Guías de remisión', icono: Route, modulo: 'GUIAS' },
      // Lubricentro.
      { id: 'ORDENES', label: 'Órdenes de servicio', icono: Wrench, modulo: 'ORDENES_SERVICIO' },
      { id: 'VEHICULOS', label: 'Vehículos', icono: Car, modulo: 'VEHICULOS' },
      // Pastelería.
      { id: 'ENCARGOS', label: 'Pedidos por encargo', icono: CakeSlice, modulo: 'ENCARGOS' },
    ],
  },
  {
    titulo: 'Inventario',
    items: [
      { id: 'PRODUCTOS', label: 'Productos', icono: Package },
      { id: 'STOCK', label: 'Stock y Lotes', icono: Boxes },
      { id: 'PROVEEDORES', label: 'Proveedores', icono: Truck },
      { id: 'DEVOLUCIONES', label: 'Devoluciones', icono: Undo2 },
      { id: 'PAQUETES', label: 'Paquetes', icono: PackageOpen, soloAdmin: true, modulo: 'PAQUETES' },
      // Pastelería / panadería.
      { id: 'PRODUCCION', label: 'Producción y mermas', icono: Wheat, modulo: 'PRODUCCION' },
    ],
  },
  {
    titulo: 'Administración',
    items: [
      { id: 'COMPROBANTES', label: 'Comprobantes', icono: Receipt },
      { id: 'REPORTES', label: 'Reportes', icono: BarChart3 },
      { id: 'GASTOS', label: 'Gastos', icono: TrendingDown, soloAdmin: true },
      { id: 'RECETAS', label: 'Libro de recetas', icono: FileCheck2, soloAdmin: true, modulo: 'RECETAS' },
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

/**
 * true si es de Almacén (rol INVENTARIO): lleva productos, stock y
 * proveedores y mira los reportes. No vende ni toca la caja.
 */
export function esAlmacen(usuario) {
  return usuario?.rol_nombre === 'INVENTARIO';
}

/**
 * true si es repartidor (dark kitchen, módulo Pedidos delivery): solo ve sus
 * entregas y cobra en la puerta los pedidos contra entrega que lleva.
 */
export function esRepartidor(usuario) {
  return usuario?.rol_nombre === 'REPARTIDOR';
}

/** Pantallas que puede abrir cada rol limitado (modo restaurante). */
export const PANTALLAS_MESERO = ['MESAS', 'CARTA'];
/** El repartidor: "Mis entregas" (la pantalla MESAS de la dark kitchen). */
export const PANTALLAS_REPARTIDOR = ['MESAS'];
export const PANTALLAS_PREPARACION = ['PREPARACION', 'CARTA'];
/** Pantallas de Almacén, en cualquier rubro (el backend bloquea el resto). */
export const PANTALLAS_ALMACEN = ['PRODUCTOS', 'STOCK', 'PROVEEDORES', 'REPORTES', 'PRODUCCION'];

/** Nombre del rol para mostrar en el menú. */
export function etiquetaRol(usuario) {
  if (usuario?.rol_id === 1) return 'Administrador';
  if (esMesero(usuario)) return 'Mesero';
  if (esPreparacion(usuario)) return 'Barra / Cocina';
  if (esAlmacen(usuario)) return 'Almacén';
  if (esRepartidor(usuario)) return 'Repartidor';
  return 'Cajero';
}

/**
 * Grupos con solo los módulos que ese usuario puede ver.
 * restaurante: el negocio atiende en mesas (muestra "Mesas").
 * etiquetas: nombres de pantalla propios del rubro (ver utils/rubros.js).
 * modulos: módulos encendidos del negocio (muestra Cotizaciones, Créditos, Guías).
 * El mesero solo ve Mesas y la Carta de hoy: toma pedidos, no cobra.
 * Barra/Cocina solo ve Preparación y la Carta de hoy (marca agotados).
 * Almacén solo ve Productos, Stock y Lotes, Proveedores y Reportes.
 */
export function gruposParaUsuario(usuario, { restaurante = false, etiquetas = {}, modulos = [] } = {}) {
  const esAdmin = usuario?.rol_id === 1;
  const mesero = restaurante && esMesero(usuario);
  const preparacion = restaurante && esPreparacion(usuario);
  const almacen = esAlmacen(usuario);
  const repartidor = restaurante && esRepartidor(usuario);
  return GRUPOS_MENU.map((g) => ({
    ...g,
    // El rubro puede renombrar pantallas ("Carta" en vez de "Productos") y
    // cambiarles el ícono (etiquetas.iconos, p. ej. la lavadora en lavandería).
    items: g.items
      .map((i) => {
        const label = typeof etiquetas[i.id] === 'string' ? etiquetas[i.id] : i.label;
        const icono = etiquetas.iconos?.[i.id] || i.icono;
        return label === i.label && icono === i.icono ? i : { ...i, label, icono };
      })
      .filter((i) => {
      if (i.soloRestaurante && !restaurante) return false;
      if (i.modulo && !modulos.includes(i.modulo)) return false;
      if (almacen) return PANTALLAS_ALMACEN.includes(i.id);
      if (repartidor) return PANTALLAS_REPARTIDOR.includes(i.id);
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
