-- Migration 000: Initial platform schema
--
-- Captured from the live Railway database (PostgreSQL 18.3) via pg_dump.
-- This establishes the baseline schema before the numbered migrations (001+)
-- are applied. Run sqlx migrate run from a clean database to reproduce the
-- full schema from this file plus migrations 001-007.
--
-- Railway-specific artifacts removed: \restrict token, drizzle migration
-- tracking schema (__drizzle_migrations), search_path override.
--
-- Dumped from database version 18.3 (Debian 18.3-1.pgdg13+1)
--
-- PostgreSQL database dump
--


-- Dumped from database version 18.3 (Debian 18.3-1.pgdg13+1)
-- Dumped by pg_dump version 18.3 (Ubuntu 18.3-1.pgdg24.04+1)

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--


--
-- Name: batch_preference_status; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.batch_preference_status AS ENUM (
    'active',
    'paused'
);


--
-- Name: campaign_signup_status; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.campaign_signup_status AS ENUM (
    'confirmed',
    'waitlist',
    'cancelled',
    'completed'
);


--
-- Name: campaign_status; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.campaign_status AS ENUM (
    'upcoming',
    'open',
    'waitlist',
    'closed',
    'completed'
);


--
-- Name: chocolate; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.chocolate AS ENUM (
    'guanaja_70',
    'caraibe_66',
    'jivara_40',
    'ivoire_blanc',
    'none'
);


--
-- Name: device_role; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.device_role AS ENUM (
    'user',
    'employee',
    'chocolatier'
);


--
-- Name: editorial_status; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.editorial_status AS ENUM (
    'draft',
    'submitted',
    'commissioned',
    'published',
    'declined'
);


--
-- Name: finish; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.finish AS ENUM (
    'plain',
    'fleur_de_sel',
    'or_fin'
);


--
-- Name: gift_tone; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.gift_tone AS ENUM (
    'warm',
    'funny',
    'poetic',
    'minimal'
);


--
-- Name: location_staff_status; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.location_staff_status AS ENUM (
    'pending',
    'approved',
    'denied'
);


--
-- Name: membership_tier; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.membership_tier AS ENUM (
    'maison',
    'reserve',
    'atelier',
    'fondateur',
    'patrimoine',
    'souverain',
    'unnamed'
);


--
-- Name: order_status; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.order_status AS ENUM (
    'pending',
    'paid',
    'preparing',
    'ready',
    'collected',
    'cancelled',
    'queued'
);


--
-- Name: social_tier; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.social_tier AS ENUM (
    'standard',
    'reserve',
    'estate'
);


--
-- Name: standing_order_frequency; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.standing_order_frequency AS ENUM (
    'weekly',
    'biweekly',
    'monthly'
);


--
-- Name: standing_order_status; Type: TYPE; Schema: public; Owner: -
--

CREATE TYPE public.standing_order_status AS ENUM (
    'active',
    'paused',
    'cancelled'
);


SET default_tablespace = '';

SET default_table_access_method = heap;

--


--
-- Name: ad_campaigns; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ad_campaigns (
    id integer NOT NULL,
    business_id integer NOT NULL,
    title text NOT NULL,
    body text NOT NULL,
    type text DEFAULT 'proximity'::text NOT NULL,
    value_cents integer NOT NULL,
    budget_cents integer DEFAULT 0 NOT NULL,
    spent_cents integer DEFAULT 0 NOT NULL,
    active boolean DEFAULT false NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: ad_campaigns_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ad_campaigns_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ad_campaigns_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ad_campaigns_id_seq OWNED BY public.ad_campaigns.id;


--
-- Name: ad_impressions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ad_impressions (
    id integer NOT NULL,
    campaign_id integer NOT NULL,
    user_id integer NOT NULL,
    accepted boolean,
    payout_cents integer NOT NULL,
    responded_at timestamp without time zone,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: ad_impressions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ad_impressions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ad_impressions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ad_impressions_id_seq OWNED BY public.ad_impressions.id;


--
-- Name: akene_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.akene_events (
    id integer NOT NULL,
    business_id integer,
    created_by_user_id integer,
    title text NOT NULL,
    description text,
    event_date timestamp with time zone,
    capacity integer DEFAULT 10 NOT NULL,
    status text DEFAULT 'inviting'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: akene_events_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.akene_events_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: akene_events_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.akene_events_id_seq OWNED BY public.akene_events.id;


--
-- Name: akene_invitations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.akene_invitations (
    id integer NOT NULL,
    event_id integer NOT NULL,
    user_id integer NOT NULL,
    sent_at timestamp with time zone DEFAULT now() NOT NULL,
    responded_at timestamp with time zone,
    status text DEFAULT 'pending'::text NOT NULL,
    expires_at timestamp with time zone
);


--
-- Name: akene_invitations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.akene_invitations_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: akene_invitations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.akene_invitations_id_seq OWNED BY public.akene_invitations.id;


--
-- Name: akene_purchases; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.akene_purchases (
    id integer NOT NULL,
    user_id integer NOT NULL,
    quantity integer DEFAULT 1 NOT NULL,
    amount_cents integer NOT NULL,
    stripe_payment_intent_id text,
    confirmed boolean DEFAULT false NOT NULL,
    purchased_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: akene_purchases_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.akene_purchases_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: akene_purchases_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.akene_purchases_id_seq OWNED BY public.akene_purchases.id;


--
-- Name: ar_notes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ar_notes (
    id integer NOT NULL,
    user_id integer NOT NULL,
    lat numeric(9,6) NOT NULL,
    lng numeric(9,6) NOT NULL,
    body text NOT NULL,
    color text DEFAULT 'amber'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone
);


--
-- Name: ar_notes_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.ar_notes_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ar_notes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.ar_notes_id_seq OWNED BY public.ar_notes.id;


--
-- Name: art_acquisitions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.art_acquisitions (
    id integer NOT NULL,
    artwork_id integer NOT NULL,
    acquisition_price_cents integer NOT NULL,
    management_fee_annual_cents integer NOT NULL,
    nfc_token_serial text,
    acquired_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: art_acquisitions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.art_acquisitions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: art_acquisitions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.art_acquisitions_id_seq OWNED BY public.art_acquisitions.id;


--
-- Name: art_auctions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.art_auctions (
    id integer NOT NULL,
    artwork_id integer NOT NULL,
    reserve_price_cents integer NOT NULL,
    starts_at timestamp with time zone NOT NULL,
    ends_at timestamp with time zone NOT NULL,
    status text DEFAULT 'active'::text NOT NULL,
    winning_bid_id integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: art_auctions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.art_auctions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: art_auctions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.art_auctions_id_seq OWNED BY public.art_auctions.id;


--
-- Name: art_bids; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.art_bids (
    id integer NOT NULL,
    auction_id integer NOT NULL,
    user_id integer NOT NULL,
    amount_cents integer NOT NULL,
    stripe_payment_intent_id text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: art_bids_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.art_bids_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: art_bids_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.art_bids_id_seq OWNED BY public.art_bids.id;


--
-- Name: art_management_fees; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.art_management_fees (
    id integer NOT NULL,
    acquisition_id integer NOT NULL,
    collector_user_id integer NOT NULL,
    amount_cents integer NOT NULL,
    due_at timestamp with time zone NOT NULL,
    paid_at timestamp with time zone,
    stripe_payment_intent_id text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: art_management_fees_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.art_management_fees_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: art_management_fees_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.art_management_fees_id_seq OWNED BY public.art_management_fees.id;


--
-- Name: art_pitches; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.art_pitches (
    id integer NOT NULL,
    user_id integer NOT NULL,
    title text NOT NULL,
    abstract text NOT NULL,
    reference_image_url text,
    status text DEFAULT 'submitted'::text NOT NULL,
    grant_amount_cents integer,
    stripe_transfer_id text,
    admin_note text,
    reviewed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: art_pitches_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.art_pitches_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: art_pitches_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.art_pitches_id_seq OWNED BY public.art_pitches.id;


--
-- Name: artworks; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.artworks (
    id integer NOT NULL,
    pitch_id integer NOT NULL,
    user_id integer NOT NULL,
    title text NOT NULL,
    media_url text NOT NULL,
    description text,
    status text DEFAULT 'posted'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: artworks_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.artworks_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: artworks_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.artworks_id_seq OWNED BY public.artworks.id;


--
-- Name: attest_challenges; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.attest_challenges (
    challenge text NOT NULL,
    expires_at timestamp with time zone DEFAULT (now() + '00:05:00'::interval) NOT NULL
);


--
-- Name: batch_preferences; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.batch_preferences (
    id integer NOT NULL,
    user_id integer NOT NULL,
    variety_id integer NOT NULL,
    chocolate public.chocolate NOT NULL,
    finish public.finish NOT NULL,
    quantity integer NOT NULL,
    location_id integer NOT NULL,
    status public.batch_preference_status DEFAULT 'active'::public.batch_preference_status NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: batch_preferences_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.batch_preferences_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: batch_preferences_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.batch_preferences_id_seq OWNED BY public.batch_preferences.id;


--
-- Name: batches; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.batches (
    id integer NOT NULL,
    location_id integer NOT NULL,
    variety_id integer NOT NULL,
    quantity_total integer NOT NULL,
    quantity_remaining integer NOT NULL,
    published boolean DEFAULT false NOT NULL,
    notes text,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    published_at timestamp without time zone,
    closed_at timestamp without time zone,
    min_quantity integer DEFAULT 4 NOT NULL,
    delivery_date date,
    cutoff_at timestamp without time zone,
    triggered_at timestamp without time zone,
    lead_days integer DEFAULT 3 NOT NULL,
    cancelled_at timestamp without time zone,
    ready_at timestamp with time zone
);


--
-- Name: batches_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.batches_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: batches_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.batches_id_seq OWNED BY public.batches.id;


--
-- Name: beacons; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.beacons (
    id integer NOT NULL,
    business_id integer NOT NULL,
    uuid text NOT NULL,
    major integer DEFAULT 1 NOT NULL,
    minor integer DEFAULT 1 NOT NULL,
    name text,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: beacons_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.beacons_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: beacons_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.beacons_id_seq OWNED BY public.beacons.id;


--
-- Name: bundle_orders; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.bundle_orders (
    id integer NOT NULL,
    bundle_id integer NOT NULL,
    user_id integer NOT NULL,
    payment_intent_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    location_id integer,
    time_slot_id integer,
    is_gift boolean DEFAULT false NOT NULL,
    gift_note text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: bundle_orders_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.bundle_orders_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: bundle_orders_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.bundle_orders_id_seq OWNED BY public.bundle_orders.id;


--
-- Name: bundle_varieties; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.bundle_varieties (
    id integer NOT NULL,
    bundle_id integer NOT NULL,
    variety_id integer NOT NULL,
    quantity integer DEFAULT 1 NOT NULL
);


--
-- Name: bundle_varieties_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.bundle_varieties_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: bundle_varieties_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.bundle_varieties_id_seq OWNED BY public.bundle_varieties.id;


--
-- Name: business_accounts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.business_accounts (
    id integer NOT NULL,
    slug text NOT NULL,
    name text NOT NULL,
    description text,
    email text NOT NULL,
    password_hash text DEFAULT ''::text NOT NULL,
    apple_id text,
    stripe_connect_account_id text,
    stripe_connect_onboarded boolean DEFAULT false NOT NULL,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: business_accounts_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.business_accounts_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: business_accounts_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.business_accounts_id_seq OWNED BY public.business_accounts.id;


--
-- Name: business_menu_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.business_menu_items (
    id integer NOT NULL,
    business_id integer NOT NULL,
    name text NOT NULL,
    description text,
    price_cents integer,
    category text DEFAULT 'main'::text NOT NULL,
    allergens jsonb DEFAULT '{}'::jsonb NOT NULL,
    tags text[] DEFAULT '{}'::text[] NOT NULL,
    is_available boolean DEFAULT true NOT NULL,
    sort_order integer DEFAULT 0 NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    calories_kcal integer,
    protein_g integer,
    carbs_g integer,
    fat_g integer,
    sugar_g integer,
    fiber_g integer
);


--
-- Name: business_menu_items_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.business_menu_items_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: business_menu_items_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.business_menu_items_id_seq OWNED BY public.business_menu_items.id;


--
-- Name: business_promotions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.business_promotions (
    id integer NOT NULL,
    business_id integer NOT NULL,
    created_by_user_id integer NOT NULL,
    title text NOT NULL,
    body text NOT NULL,
    fee_per_read_cents integer DEFAULT 200 NOT NULL,
    budget_cents integer NOT NULL,
    spent_cents integer DEFAULT 0 NOT NULL,
    status text DEFAULT 'active'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: business_promotions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.business_promotions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: business_promotions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.business_promotions_id_seq OWNED BY public.business_promotions.id;


--
-- Name: business_proposals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.business_proposals (
    id integer NOT NULL,
    proposed_by_user_id integer NOT NULL,
    proposed_by_name text,
    business_name text NOT NULL,
    business_address text,
    business_type text DEFAULT 'partner'::text NOT NULL,
    business_email text,
    instagram_handle text,
    note text,
    claim_token text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    claimed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: business_proposals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.business_proposals_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: business_proposals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.business_proposals_id_seq OWNED BY public.business_proposals.id;


--
-- Name: business_visits; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.business_visits (
    id integer NOT NULL,
    business_id integer NOT NULL,
    contracted_user_id integer NOT NULL,
    visitor_user_id integer,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: business_visits_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.business_visits_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: business_visits_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.business_visits_id_seq OWNED BY public.business_visits.id;


--
-- Name: businesses; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.businesses (
    id integer NOT NULL,
    name text NOT NULL,
    type text NOT NULL,
    address text NOT NULL,
    city text NOT NULL,
    hours text,
    contact text,
    latitude numeric(10,7),
    longitude numeric(10,7),
    launched_at timestamp without time zone NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    description text,
    instagram_handle text,
    neighbourhood text,
    starts_at timestamp without time zone,
    ends_at timestamp without time zone,
    dj_name text,
    organizer_note text,
    capacity integer,
    entrance_fee_cents integer,
    is_audition boolean DEFAULT false NOT NULL,
    audition_status text,
    partner_business_id integer,
    host_user_id integer,
    checkin_token text,
    location_type text DEFAULT 'collection'::text NOT NULL,
    partner_name text,
    operating_cost_cents integer,
    founding_patron_id integer,
    founding_term_ends_at timestamp without time zone,
    inaugurated_at timestamp without time zone,
    approved_by_admin boolean DEFAULT false NOT NULL,
    proximity_message text,
    venture_id integer,
    toilet_fee_cents integer DEFAULT 150 NOT NULL,
    has_toilet boolean DEFAULT false NOT NULL,
    beacon_uuid text,
    allows_walkin boolean DEFAULT false NOT NULL,
    sticker_concept text,
    sticker_emoji text,
    sticker_image_url text,
    food_popup_status text DEFAULT 'announced'::text NOT NULL,
    min_orders_to_confirm integer,
    confirmed_at timestamp with time zone
);


--
-- Name: businesses_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.businesses_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: businesses_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.businesses_id_seq OWNED BY public.businesses.id;


--
-- Name: campaign_commissions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.campaign_commissions (
    id integer NOT NULL,
    popup_id integer NOT NULL,
    commissioner_user_id integer NOT NULL,
    stripe_payment_intent_id text,
    invited_user_ids jsonb DEFAULT '[]'::jsonb NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: campaign_commissions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.campaign_commissions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: campaign_commissions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.campaign_commissions_id_seq OWNED BY public.campaign_commissions.id;


--
-- Name: campaign_signups; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.campaign_signups (
    id integer NOT NULL,
    campaign_id integer NOT NULL,
    user_id integer NOT NULL,
    waitlist boolean DEFAULT false NOT NULL,
    status public.campaign_signup_status DEFAULT 'confirmed'::public.campaign_signup_status NOT NULL,
    signed_up_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: campaign_signups_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.campaign_signups_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: campaign_signups_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.campaign_signups_id_seq OWNED BY public.campaign_signups.id;


--
-- Name: campaigns; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.campaigns (
    id integer NOT NULL,
    title text NOT NULL,
    concept text NOT NULL,
    salon_id integer NOT NULL,
    paying_client_id integer,
    date timestamp without time zone NOT NULL,
    total_spots integer NOT NULL,
    spots_remaining integer NOT NULL,
    status public.campaign_status DEFAULT 'upcoming'::public.campaign_status NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: campaigns_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.campaigns_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: campaigns_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.campaigns_id_seq OWNED BY public.campaigns.id;


--
-- Name: co_scans; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.co_scans (
    id integer NOT NULL,
    variety_id integer NOT NULL,
    initiator_code text NOT NULL,
    user_id_a integer NOT NULL,
    user_id_b integer,
    scanned_at timestamp with time zone DEFAULT now()
);


--
-- Name: co_scans_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.co_scans_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: co_scans_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.co_scans_id_seq OWNED BY public.co_scans.id;


--
-- Name: collectif_challenges; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.collectif_challenges (
    id integer NOT NULL,
    title text NOT NULL,
    description text,
    challenge_type text DEFAULT 'scan_farms'::text NOT NULL,
    target_count integer DEFAULT 3 NOT NULL,
    started_at timestamp with time zone DEFAULT now(),
    ends_at timestamp with time zone
);


--
-- Name: collectif_challenges_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.collectif_challenges_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: collectif_challenges_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.collectif_challenges_id_seq OWNED BY public.collectif_challenges.id;


--
-- Name: collectif_commitments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.collectif_commitments (
    id integer NOT NULL,
    collectif_id integer NOT NULL,
    user_id integer NOT NULL,
    quantity integer DEFAULT 1 NOT NULL,
    amount_paid_cents integer NOT NULL,
    payment_intent_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    committed_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: collectif_commitments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.collectif_commitments_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: collectif_commitments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.collectif_commitments_id_seq OWNED BY public.collectif_commitments.id;


--
-- Name: collectifs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.collectifs (
    id integer NOT NULL,
    created_by integer NOT NULL,
    business_id integer,
    business_name text NOT NULL,
    collectif_type text DEFAULT 'product'::text NOT NULL,
    title text NOT NULL,
    description text,
    proposed_discount_pct integer NOT NULL,
    price_cents integer NOT NULL,
    proposed_venue text,
    proposed_date text,
    target_quantity integer NOT NULL,
    current_quantity integer DEFAULT 0 NOT NULL,
    deadline timestamp with time zone NOT NULL,
    status text DEFAULT 'open'::text NOT NULL,
    business_response text DEFAULT 'pending'::text,
    business_response_note text,
    responded_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    milestone_75_sent boolean DEFAULT false NOT NULL,
    milestone_50_sent boolean DEFAULT false NOT NULL
);


--
-- Name: collectifs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.collectifs_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: collectifs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.collectifs_id_seq OWNED BY public.collectifs.id;


--
-- Name: community_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.community_events (
    id integer NOT NULL,
    event_date date NOT NULL,
    operator_names text NOT NULL,
    people_fed integer DEFAULT 0 NOT NULL,
    location text,
    description text,
    photo_url text,
    fund_raised_cents integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: community_events_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.community_events_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: community_events_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.community_events_id_seq OWNED BY public.community_events.id;


--
-- Name: community_fund; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.community_fund (
    id integer NOT NULL,
    balance_cents integer DEFAULT 0 NOT NULL,
    total_raised_cents integer DEFAULT 0 NOT NULL,
    threshold_cents integer DEFAULT 50000 NOT NULL,
    popup_count integer DEFAULT 0 NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: community_fund_contributions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.community_fund_contributions (
    id integer NOT NULL,
    user_id integer,
    amount_cents integer DEFAULT 200 NOT NULL,
    order_type text NOT NULL,
    stripe_payment_intent_id text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: community_fund_contributions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.community_fund_contributions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: community_fund_contributions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.community_fund_contributions_id_seq OWNED BY public.community_fund_contributions.id;


--
-- Name: community_fund_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.community_fund_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: community_fund_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.community_fund_id_seq OWNED BY public.community_fund.id;


--
-- Name: community_popup_interest; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.community_popup_interest (
    id integer NOT NULL,
    user_id integer NOT NULL,
    business_id integer,
    concept text,
    note text,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: community_popup_interest_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.community_popup_interest_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: community_popup_interest_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.community_popup_interest_id_seq OWNED BY public.community_popup_interest.id;


--
-- Name: connections; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.connections (
    id integer NOT NULL,
    user_a_id integer NOT NULL,
    user_b_id integer NOT NULL,
    connected_at timestamp with time zone DEFAULT now() NOT NULL,
    met_at timestamp with time zone
);


--
-- Name: connections_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.connections_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: connections_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.connections_id_seq OWNED BY public.connections.id;


--
-- Name: contract_requests; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.contract_requests (
    id integer NOT NULL,
    business_id integer NOT NULL,
    description text,
    desired_start text,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: contract_requests_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.contract_requests_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: contract_requests_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.contract_requests_id_seq OWNED BY public.contract_requests.id;


--
-- Name: conversation_archives; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.conversation_archives (
    id integer NOT NULL,
    user_id integer NOT NULL,
    other_user_id integer NOT NULL,
    archived_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: conversation_archives_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.conversation_archives_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: conversation_archives_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.conversation_archives_id_seq OWNED BY public.conversation_archives.id;


--
-- Name: corporate_accounts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.corporate_accounts (
    id integer NOT NULL,
    name text NOT NULL,
    billing_email text NOT NULL,
    admin_user_id integer NOT NULL,
    stripe_customer_id text,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: corporate_accounts_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.corporate_accounts_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: corporate_accounts_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.corporate_accounts_id_seq OWNED BY public.corporate_accounts.id;


--
-- Name: corporate_members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.corporate_members (
    id integer NOT NULL,
    corporate_id integer NOT NULL,
    user_id integer NOT NULL,
    standing_order_id integer,
    invited_by_user_id integer,
    joined_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: corporate_members_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.corporate_members_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: corporate_members_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.corporate_members_id_seq OWNED BY public.corporate_members.id;


--
-- Name: credit_transactions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.credit_transactions (
    id integer NOT NULL,
    from_user_id integer,
    to_user_id integer NOT NULL,
    amount_cents integer NOT NULL,
    type text NOT NULL,
    stripe_payment_intent_id text,
    note text,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: credit_transactions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.credit_transactions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: credit_transactions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.credit_transactions_id_seq OWNED BY public.credit_transactions.id;


--
-- Name: date_invitations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.date_invitations (
    id integer NOT NULL,
    offer_id integer NOT NULL,
    user_id integer NOT NULL,
    sent_at timestamp with time zone DEFAULT now() NOT NULL,
    opened_at timestamp with time zone,
    responded_at timestamp with time zone,
    status text DEFAULT 'pending'::text NOT NULL,
    fee_cents integer DEFAULT 500 NOT NULL
);


--
-- Name: date_invitations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.date_invitations_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: date_invitations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.date_invitations_id_seq OWNED BY public.date_invitations.id;


--
-- Name: date_matches; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.date_matches (
    id integer NOT NULL,
    offer_id integer NOT NULL,
    user_a_id integer NOT NULL,
    user_b_id integer NOT NULL,
    matched_at timestamp with time zone DEFAULT now() NOT NULL,
    status text DEFAULT 'confirmed'::text NOT NULL
);


--
-- Name: date_matches_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.date_matches_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: date_matches_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.date_matches_id_seq OWNED BY public.date_matches.id;


--
-- Name: date_offers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.date_offers (
    id integer NOT NULL,
    business_id integer,
    created_by_user_id integer NOT NULL,
    title text NOT NULL,
    description text,
    event_date timestamp with time zone NOT NULL,
    seats integer DEFAULT 2 NOT NULL,
    budget_cents integer NOT NULL,
    fee_per_view_cents integer DEFAULT 500 NOT NULL,
    status text DEFAULT 'active'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: date_offers_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.date_offers_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: date_offers_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.date_offers_id_seq OWNED BY public.date_offers.id;


--
-- Name: device_attestations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.device_attestations (
    id integer NOT NULL,
    key_id text NOT NULL,
    attestation text NOT NULL,
    challenge text,
    user_id integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    hmac_key text,
    public_key text
);


--
-- Name: device_attestations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.device_attestations_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: device_attestations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.device_attestations_id_seq OWNED BY public.device_attestations.id;


--
-- Name: device_pairing_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.device_pairing_tokens (
    id integer NOT NULL,
    token text NOT NULL,
    user_id integer NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: device_pairing_tokens_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.device_pairing_tokens_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: device_pairing_tokens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.device_pairing_tokens_id_seq OWNED BY public.device_pairing_tokens.id;


--
-- Name: devices; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.devices (
    id integer NOT NULL,
    device_address text NOT NULL,
    user_id integer NOT NULL,
    role public.device_role DEFAULT 'user'::public.device_role NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: devices_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.devices_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: devices_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.devices_id_seq OWNED BY public.devices.id;


--
-- Name: dj_offers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.dj_offers (
    id integer NOT NULL,
    popup_id integer NOT NULL,
    dj_user_id integer NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    allocation_boxes integer DEFAULT 0 NOT NULL,
    organizer_note text,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: dj_offers_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.dj_offers_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: dj_offers_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.dj_offers_id_seq OWNED BY public.dj_offers.id;


--
-- Name: drop_claims; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.drop_claims (
    id integer NOT NULL,
    drop_id integer NOT NULL,
    user_id integer NOT NULL,
    quantity integer DEFAULT 1 NOT NULL,
    payment_intent_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    claimed_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: drop_claims_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.drop_claims_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: drop_claims_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.drop_claims_id_seq OWNED BY public.drop_claims.id;


--
-- Name: drop_waitlist; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.drop_waitlist (
    id integer NOT NULL,
    drop_id integer NOT NULL,
    user_id integer NOT NULL,
    joined_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: drop_waitlist_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.drop_waitlist_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: drop_waitlist_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.drop_waitlist_id_seq OWNED BY public.drop_waitlist.id;


--
-- Name: drops; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.drops (
    id integer NOT NULL,
    title text NOT NULL,
    price_cents integer,
    active boolean DEFAULT true NOT NULL,
    variety_id integer,
    upcoming_drop_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: drops_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.drops_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: drops_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.drops_id_seq OWNED BY public.drops.id;


--
-- Name: editorial_pieces; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.editorial_pieces (
    id integer NOT NULL,
    author_user_id integer NOT NULL,
    title text NOT NULL,
    body text NOT NULL,
    status public.editorial_status DEFAULT 'draft'::public.editorial_status NOT NULL,
    commission_cents integer,
    published_at timestamp without time zone,
    editor_note text,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    updated_at timestamp without time zone DEFAULT now() NOT NULL,
    tag text
);


--
-- Name: editorial_pieces_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.editorial_pieces_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: editorial_pieces_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.editorial_pieces_id_seq OWNED BY public.editorial_pieces.id;


--
-- Name: employment_contracts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.employment_contracts (
    id integer NOT NULL,
    business_id integer NOT NULL,
    user_id integer NOT NULL,
    starts_at timestamp without time zone NOT NULL,
    ends_at timestamp without time zone NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    note text,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: employment_contracts_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.employment_contracts_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: employment_contracts_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.employment_contracts_id_seq OWNED BY public.employment_contracts.id;


--
-- Name: evening_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.evening_tokens (
    id integer NOT NULL,
    booking_id integer NOT NULL,
    user_a_id integer NOT NULL,
    user_b_id integer NOT NULL,
    business_id integer NOT NULL,
    offer_id integer NOT NULL,
    window_closes_at timestamp without time zone NOT NULL,
    user_a_confirmed boolean DEFAULT false NOT NULL,
    user_b_confirmed boolean DEFAULT false NOT NULL,
    minted_at timestamp without time zone,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    memory_asked_at timestamp with time zone,
    user_a_declined boolean DEFAULT false NOT NULL,
    user_b_declined boolean DEFAULT false NOT NULL,
    user_a_asked_at timestamp with time zone,
    user_b_asked_at timestamp with time zone
);


--
-- Name: evening_tokens_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.evening_tokens_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: evening_tokens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.evening_tokens_id_seq OWNED BY public.evening_tokens.id;


--
-- Name: explicit_portals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.explicit_portals (
    id integer NOT NULL,
    user_id integer NOT NULL,
    opted_in boolean DEFAULT false NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: explicit_portals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.explicit_portals_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: explicit_portals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.explicit_portals_id_seq OWNED BY public.explicit_portals.id;


--
-- Name: farm_visit_bookings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.farm_visit_bookings (
    id integer NOT NULL,
    visit_id integer NOT NULL,
    user_id integer NOT NULL,
    payment_intent_id text,
    status text DEFAULT 'confirmed'::text NOT NULL,
    booked_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: farm_visit_bookings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.farm_visit_bookings_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: farm_visit_bookings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.farm_visit_bookings_id_seq OWNED BY public.farm_visit_bookings.id;


--
-- Name: farm_visits; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.farm_visits (
    id integer NOT NULL,
    farm_name text NOT NULL,
    location text NOT NULL,
    visit_date date NOT NULL,
    max_participants integer DEFAULT 12 NOT NULL,
    price_cents integer DEFAULT 0 NOT NULL,
    description text,
    status text DEFAULT 'open'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: farm_visits_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.farm_visits_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: farm_visits_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.farm_visits_id_seq OWNED BY public.farm_visits.id;


--
-- Name: fraise_business_sessions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.fraise_business_sessions (
    id integer NOT NULL,
    business_id integer NOT NULL,
    token text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: fraise_business_sessions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.fraise_business_sessions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: fraise_business_sessions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.fraise_business_sessions_id_seq OWNED BY public.fraise_business_sessions.id;


--
-- Name: fraise_businesses; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.fraise_businesses (
    id integer NOT NULL,
    slug text NOT NULL,
    name text NOT NULL,
    description text,
    email text NOT NULL,
    password_hash text,
    stripe_connect_account_id text,
    stripe_connect_onboarded boolean DEFAULT false NOT NULL,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    lat double precision,
    lng double precision,
    member_id integer,
    status text DEFAULT 'pending'::text NOT NULL,
    category text,
    address text
);


--
-- Name: fraise_businesses_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.fraise_businesses_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: fraise_businesses_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.fraise_businesses_id_seq OWNED BY public.fraise_businesses.id;


--
-- Name: fraise_claims; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.fraise_claims (
    id integer NOT NULL,
    member_id integer NOT NULL,
    event_id integer NOT NULL,
    status text DEFAULT 'claimed'::text NOT NULL,
    confirm_token text,
    confirmed_at timestamp with time zone,
    declined_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    stripe_payment_intent_id text,
    amount_paid_cents integer,
    stripe_refund_id text
);


--
-- Name: fraise_claims_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.fraise_claims_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: fraise_claims_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.fraise_claims_id_seq OWNED BY public.fraise_claims.id;


--
-- Name: fraise_credit_purchases; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.fraise_credit_purchases (
    id integer NOT NULL,
    member_id integer NOT NULL,
    credits integer NOT NULL,
    amount_cents integer NOT NULL,
    stripe_payment_intent_id text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: fraise_credit_purchases_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.fraise_credit_purchases_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: fraise_credit_purchases_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.fraise_credit_purchases_id_seq OWNED BY public.fraise_credit_purchases.id;


--
-- Name: fraise_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.fraise_events (
    id integer NOT NULL,
    business_id integer NOT NULL,
    title text NOT NULL,
    description text,
    price_cents integer DEFAULT 12000 NOT NULL,
    min_seats integer DEFAULT 6 NOT NULL,
    max_seats integer DEFAULT 20 NOT NULL,
    seats_claimed integer DEFAULT 0 NOT NULL,
    status text DEFAULT 'open'::text NOT NULL,
    event_date text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    location_text text,
    lat double precision,
    lng double precision,
    scheduled_at timestamp with time zone
);


--
-- Name: fraise_events_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.fraise_events_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: fraise_events_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.fraise_events_id_seq OWNED BY public.fraise_events.id;


--
-- Name: fraise_interest; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.fraise_interest (
    id integer NOT NULL,
    business_id integer NOT NULL,
    name text NOT NULL,
    email text NOT NULL,
    fraise_member_id integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: fraise_interest_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.fraise_interest_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: fraise_interest_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.fraise_interest_id_seq OWNED BY public.fraise_interest.id;


--
-- Name: fraise_invitations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.fraise_invitations (
    id integer NOT NULL,
    event_id integer NOT NULL,
    member_id integer NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    responded_at timestamp with time zone,
    confirm_token text
);


--
-- Name: fraise_invitations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.fraise_invitations_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: fraise_invitations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.fraise_invitations_id_seq OWNED BY public.fraise_invitations.id;


--
-- Name: fraise_member_resets; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.fraise_member_resets (
    id integer NOT NULL,
    member_id integer NOT NULL,
    code text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    used_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: fraise_member_resets_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.fraise_member_resets_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: fraise_member_resets_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.fraise_member_resets_id_seq OWNED BY public.fraise_member_resets.id;


--
-- Name: fraise_member_sessions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.fraise_member_sessions (
    id integer NOT NULL,
    member_id integer NOT NULL,
    token text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: fraise_member_sessions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.fraise_member_sessions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: fraise_member_sessions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.fraise_member_sessions_id_seq OWNED BY public.fraise_member_sessions.id;


--
-- Name: fraise_members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.fraise_members (
    id integer NOT NULL,
    name text NOT NULL,
    email text NOT NULL,
    password_hash text,
    credit_balance integer DEFAULT 0 NOT NULL,
    credits_purchased integer DEFAULT 0 NOT NULL,
    stripe_customer_id text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    push_token text,
    apple_sub text
);


--
-- Name: fraise_members_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.fraise_members_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: fraise_members_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.fraise_members_id_seq OWNED BY public.fraise_members.id;


--
-- Name: fraise_messages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.fraise_messages (
    id integer NOT NULL,
    user_id integer NOT NULL,
    from_email text NOT NULL,
    from_name text,
    subject text,
    body text NOT NULL,
    received_at timestamp with time zone DEFAULT now() NOT NULL,
    read_at timestamp with time zone
);


--
-- Name: fraise_messages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.fraise_messages_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: fraise_messages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.fraise_messages_id_seq OWNED BY public.fraise_messages.id;


--
-- Name: fund_contributions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.fund_contributions (
    id integer NOT NULL,
    from_user_id integer,
    to_user_id integer NOT NULL,
    amount_cents integer NOT NULL,
    stripe_payment_intent_id text,
    note text,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: fund_contributions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.fund_contributions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: fund_contributions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.fund_contributions_id_seq OWNED BY public.fund_contributions.id;


--
-- Name: gift_registry; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.gift_registry (
    id integer NOT NULL,
    user_id integer NOT NULL,
    variety_id integer NOT NULL,
    variety_name text,
    added_at timestamp with time zone DEFAULT now()
);


--
-- Name: gift_registry_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.gift_registry_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: gift_registry_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.gift_registry_id_seq OWNED BY public.gift_registry.id;


--
-- Name: gifts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.gifts (
    id integer NOT NULL,
    sender_user_id integer NOT NULL,
    recipient_email text,
    gift_type text NOT NULL,
    amount_cents integer NOT NULL,
    claim_token text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    payment_intent_id text,
    shipping_name text,
    shipping_address text,
    shipping_city text,
    shipping_province text,
    shipping_postal_code text,
    claimed_by_user_id integer,
    claimed_at timestamp without time zone,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    sticker_business_id integer,
    business_revenue_cents integer,
    is_outreach boolean DEFAULT false NOT NULL,
    recipient_phone text
);


--
-- Name: gifts_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.gifts_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: gifts_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.gifts_id_seq OWNED BY public.gifts.id;


--
-- Name: greenhouse_funding; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.greenhouse_funding (
    id integer NOT NULL,
    greenhouse_id integer NOT NULL,
    user_id integer NOT NULL,
    amount_cents integer NOT NULL,
    years integer NOT NULL,
    stripe_payment_intent_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: greenhouse_funding_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.greenhouse_funding_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: greenhouse_funding_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.greenhouse_funding_id_seq OWNED BY public.greenhouse_funding.id;


--
-- Name: greenhouses; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.greenhouses (
    id integer NOT NULL,
    name text NOT NULL,
    location text NOT NULL,
    description text,
    status text DEFAULT 'funding'::text NOT NULL,
    funding_goal_cents integer NOT NULL,
    funded_cents integer DEFAULT 0 NOT NULL,
    founding_patron_id integer,
    founding_years integer,
    founding_term_ends_at timestamp without time zone,
    opened_at timestamp without time zone,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    approved_by_admin boolean DEFAULT false NOT NULL
);


--
-- Name: greenhouses_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.greenhouses_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: greenhouses_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.greenhouses_id_seq OWNED BY public.greenhouses.id;


--
-- Name: harvest_logs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.harvest_logs (
    id integer NOT NULL,
    supplier_user_id integer NOT NULL,
    variety_id integer,
    variety_name_freeform text,
    harvest_date date NOT NULL,
    quantity_kg numeric(10,2),
    notes text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: harvest_logs_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.harvest_logs_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: harvest_logs_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.harvest_logs_id_seq OWNED BY public.harvest_logs.id;


--
-- Name: health_profiles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.health_profiles (
    id integer NOT NULL,
    user_id integer NOT NULL,
    dietary_restrictions text[] DEFAULT '{}'::text[] NOT NULL,
    allergens jsonb DEFAULT '{}'::jsonb,
    biometric_markers jsonb DEFAULT '{}'::jsonb,
    flavor_profile jsonb DEFAULT '{}'::jsonb,
    caloric_needs integer,
    dorotka_note text,
    last_reading_at timestamp without time zone,
    updated_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: health_profiles_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.health_profiles_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: health_profiles_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.health_profiles_id_seq OWNED BY public.health_profiles.id;


--
-- Name: id_attestation_log; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.id_attestation_log (
    id integer NOT NULL,
    user_id integer NOT NULL,
    attested_by integer NOT NULL,
    attested_at timestamp with time zone DEFAULT now() NOT NULL,
    outcome text DEFAULT 'pending'::text NOT NULL,
    stripe_session_id text,
    id_verified_name text,
    id_verified_dob text
);


--
-- Name: id_attestation_log_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.id_attestation_log_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: id_attestation_log_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.id_attestation_log_id_seq OWNED BY public.id_attestation_log.id;


--
-- Name: itineraries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.itineraries (
    id integer NOT NULL,
    user_id integer NOT NULL,
    title text NOT NULL,
    description text,
    status text DEFAULT 'active'::text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    updated_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: itineraries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.itineraries_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: itineraries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.itineraries_id_seq OWNED BY public.itineraries.id;


--
-- Name: itinerary_destinations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.itinerary_destinations (
    id integer NOT NULL,
    itinerary_id integer NOT NULL,
    business_id integer,
    place_name text NOT NULL,
    city text NOT NULL,
    country text NOT NULL,
    lat numeric(10,7),
    lng numeric(10,7),
    arrival_date text,
    departure_date text,
    notes text,
    sort_order integer DEFAULT 0 NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: itinerary_destinations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.itinerary_destinations_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: itinerary_destinations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.itinerary_destinations_id_seq OWNED BY public.itinerary_destinations.id;


--
-- Name: itinerary_proposals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.itinerary_proposals (
    id integer NOT NULL,
    user_id integer NOT NULL,
    business_id integer NOT NULL,
    itinerary_id integer,
    destination_id integer,
    title text NOT NULL,
    body text NOT NULL,
    value_cents integer NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    responded_at timestamp without time zone,
    visit_confirmed_at timestamp without time zone
);


--
-- Name: itinerary_proposals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.itinerary_proposals_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: itinerary_proposals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.itinerary_proposals_id_seq OWNED BY public.itinerary_proposals.id;


--
-- Name: job_applications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.job_applications (
    id integer NOT NULL,
    job_id integer NOT NULL,
    applicant_id integer NOT NULL,
    status text DEFAULT 'applied'::text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: job_applications_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.job_applications_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: job_applications_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.job_applications_id_seq OWNED BY public.job_applications.id;


--
-- Name: job_interviews; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.job_interviews (
    id integer NOT NULL,
    application_id integer NOT NULL,
    scheduled_at timestamp without time zone NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: job_interviews_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.job_interviews_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: job_interviews_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.job_interviews_id_seq OWNED BY public.job_interviews.id;


--
-- Name: job_ledger_entries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.job_ledger_entries (
    id integer NOT NULL,
    application_id integer NOT NULL,
    employer_statement text,
    candidate_statement text,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: job_ledger_entries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.job_ledger_entries_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: job_ledger_entries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.job_ledger_entries_id_seq OWNED BY public.job_ledger_entries.id;


--
-- Name: job_postings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.job_postings (
    id integer NOT NULL,
    business_id integer NOT NULL,
    title text NOT NULL,
    description text,
    pay_cents integer NOT NULL,
    pay_type text DEFAULT 'hourly'::text NOT NULL,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: job_postings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.job_postings_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: job_postings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.job_postings_id_seq OWNED BY public.job_postings.id;


--
-- Name: key_challenges; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.key_challenges (
    id integer NOT NULL,
    user_id integer NOT NULL,
    challenge text NOT NULL,
    expires_at timestamp without time zone NOT NULL,
    used boolean DEFAULT false NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: key_challenges_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.key_challenges_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: key_challenges_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.key_challenges_id_seq OWNED BY public.key_challenges.id;


--
-- Name: kommune_assignments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.kommune_assignments (
    id integer NOT NULL,
    name text NOT NULL,
    neighbourhood text NOT NULL,
    note text,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: kommune_assignments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.kommune_assignments_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: kommune_assignments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.kommune_assignments_id_seq OWNED BY public.kommune_assignments.id;


--
-- Name: kommune_flavour_suggestions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.kommune_flavour_suggestions (
    id integer NOT NULL,
    suggestion text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: kommune_flavour_suggestions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.kommune_flavour_suggestions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: kommune_flavour_suggestions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.kommune_flavour_suggestions_id_seq OWNED BY public.kommune_flavour_suggestions.id;


--
-- Name: kommune_press_applications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.kommune_press_applications (
    id integer NOT NULL,
    name text NOT NULL,
    email text NOT NULL,
    note text,
    status text DEFAULT 'pending'::text NOT NULL,
    personal_code text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    user_id integer
);


--
-- Name: kommune_press_applications_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.kommune_press_applications_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: kommune_press_applications_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.kommune_press_applications_id_seq OWNED BY public.kommune_press_applications.id;


--
-- Name: kommune_ratings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.kommune_ratings (
    id integer NOT NULL,
    item_name text NOT NULL,
    rating integer NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT kommune_ratings_rating_check CHECK (((rating >= 1) AND (rating <= 5)))
);


--
-- Name: kommune_ratings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.kommune_ratings_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: kommune_ratings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.kommune_ratings_id_seq OWNED BY public.kommune_ratings.id;


--
-- Name: kommune_reservations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.kommune_reservations (
    id integer NOT NULL,
    name text NOT NULL,
    size integer NOT NULL,
    date text NOT NULL,
    "time" text NOT NULL,
    note text DEFAULT ''::text NOT NULL,
    preorder text DEFAULT ''::text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    email text,
    total_cents integer DEFAULT 0 NOT NULL,
    stripe_payment_intent_id text,
    order_json jsonb,
    event_id integer
);


--
-- Name: kommune_reservations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.kommune_reservations_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: kommune_reservations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.kommune_reservations_id_seq OWNED BY public.kommune_reservations.id;


--
-- Name: legitimacy_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.legitimacy_events (
    id integer NOT NULL,
    user_id integer NOT NULL,
    event_type text NOT NULL,
    weight integer NOT NULL,
    business_id integer,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: legitimacy_events_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.legitimacy_events_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: legitimacy_events_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.legitimacy_events_id_seq OWNED BY public.legitimacy_events.id;


--
-- Name: location_funding; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.location_funding (
    id integer NOT NULL,
    business_id integer NOT NULL,
    user_id integer NOT NULL,
    amount_cents integer NOT NULL,
    stripe_payment_intent_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: location_funding_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.location_funding_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: location_funding_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.location_funding_id_seq OWNED BY public.location_funding.id;


--
-- Name: location_staff; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.location_staff (
    id integer NOT NULL,
    user_id integer NOT NULL,
    location_id integer NOT NULL,
    status public.location_staff_status DEFAULT 'pending'::public.location_staff_status NOT NULL,
    requested_at timestamp without time zone DEFAULT now() NOT NULL,
    reviewed_at timestamp without time zone
);


--
-- Name: location_staff_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.location_staff_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: location_staff_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.location_staff_id_seq OWNED BY public.location_staff.id;


--
-- Name: locations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.locations (
    id integer NOT NULL,
    name text NOT NULL,
    address text NOT NULL,
    active boolean DEFAULT true NOT NULL,
    staff_pin text,
    allows_walkin boolean DEFAULT false NOT NULL,
    beacon_uuid text,
    business_id integer
);


--
-- Name: locations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.locations_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: locations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.locations_id_seq OWNED BY public.locations.id;


--
-- Name: market_dates; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.market_dates (
    id integer NOT NULL,
    name text NOT NULL,
    location text NOT NULL,
    address text NOT NULL,
    latitude numeric(9,6),
    longitude numeric(9,6),
    starts_at timestamp with time zone NOT NULL,
    ends_at timestamp with time zone NOT NULL,
    status text DEFAULT 'scheduled'::text NOT NULL,
    notes text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: market_dates_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.market_dates_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: market_dates_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.market_dates_id_seq OWNED BY public.market_dates.id;


--
-- Name: market_listings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.market_listings (
    id integer NOT NULL,
    vendor_id integer NOT NULL,
    name text NOT NULL,
    description text,
    category text DEFAULT 'other'::text NOT NULL,
    unit_type text DEFAULT 'per_item'::text NOT NULL,
    unit_label text DEFAULT 'each'::text NOT NULL,
    price_cents integer NOT NULL,
    stock_quantity integer DEFAULT 0 NOT NULL,
    tags text[] DEFAULT '{}'::text[],
    available_from timestamp with time zone NOT NULL,
    available_until timestamp with time zone NOT NULL,
    is_available boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: market_listings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.market_listings_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: market_listings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.market_listings_id_seq OWNED BY public.market_listings.id;


--
-- Name: market_order_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.market_order_items (
    id integer NOT NULL,
    order_id integer NOT NULL,
    listing_id integer NOT NULL,
    listing_name text NOT NULL,
    quantity integer DEFAULT 1 NOT NULL,
    unit_price_cents integer NOT NULL
);


--
-- Name: market_order_items_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.market_order_items_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: market_order_items_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.market_order_items_id_seq OWNED BY public.market_order_items.id;


--
-- Name: market_orders; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.market_orders (
    id integer NOT NULL,
    market_date_id integer NOT NULL,
    stall_id integer NOT NULL,
    product_id integer NOT NULL,
    buyer_user_id integer NOT NULL,
    quantity integer DEFAULT 1 NOT NULL,
    amount_paid_cents integer NOT NULL,
    payment_intent_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    collected_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: market_orders_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.market_orders_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: market_orders_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.market_orders_id_seq OWNED BY public.market_orders.id;


--
-- Name: market_orders_v2; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.market_orders_v2 (
    id integer NOT NULL,
    user_id integer NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    total_cents integer DEFAULT 0 NOT NULL,
    nfc_collected_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: market_orders_v2_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.market_orders_v2_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: market_orders_v2_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.market_orders_v2_id_seq OWNED BY public.market_orders_v2.id;


--
-- Name: market_products; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.market_products (
    id integer NOT NULL,
    stall_id integer NOT NULL,
    name text NOT NULL,
    description text,
    price_cents integer NOT NULL,
    unit text DEFAULT 'unit'::text NOT NULL,
    stock_quantity integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: market_products_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.market_products_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: market_products_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.market_products_id_seq OWNED BY public.market_products.id;


--
-- Name: market_stalls; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.market_stalls (
    id integer NOT NULL,
    market_date_id integer NOT NULL,
    vendor_user_id integer,
    vendor_name text NOT NULL,
    description text,
    confirmed boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: market_stalls_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.market_stalls_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: market_stalls_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.market_stalls_id_seq OWNED BY public.market_stalls.id;


--
-- Name: market_vendors; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.market_vendors (
    id integer NOT NULL,
    user_id integer NOT NULL,
    name text NOT NULL,
    description text,
    instagram_handle text,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: market_vendors_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.market_vendors_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: market_vendors_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.market_vendors_id_seq OWNED BY public.market_vendors.id;


--
-- Name: meeting_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.meeting_tokens (
    id integer NOT NULL,
    user_id integer NOT NULL,
    token text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone DEFAULT (now() + '00:10:00'::interval) NOT NULL,
    used boolean DEFAULT false NOT NULL
);


--
-- Name: meeting_tokens_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.meeting_tokens_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: meeting_tokens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.meeting_tokens_id_seq OWNED BY public.meeting_tokens.id;


--
-- Name: membership_funds; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.membership_funds (
    id integer NOT NULL,
    user_id integer NOT NULL,
    balance_cents integer DEFAULT 0 NOT NULL,
    cycle_start timestamp without time zone DEFAULT now() NOT NULL,
    updated_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: membership_funds_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.membership_funds_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: membership_funds_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.membership_funds_id_seq OWNED BY public.membership_funds.id;


--
-- Name: membership_waitlist; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.membership_waitlist (
    id integer NOT NULL,
    user_id integer NOT NULL,
    tier public.membership_tier NOT NULL,
    message text,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: membership_waitlist_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.membership_waitlist_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: membership_waitlist_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.membership_waitlist_id_seq OWNED BY public.membership_waitlist.id;


--
-- Name: memberships; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.memberships (
    id integer NOT NULL,
    user_id integer NOT NULL,
    tier public.membership_tier NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    started_at timestamp without time zone,
    renews_at timestamp without time zone,
    amount_cents integer NOT NULL,
    stripe_payment_intent_id text,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    renewal_notified_at timestamp without time zone
);


--
-- Name: memberships_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.memberships_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: memberships_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.memberships_id_seq OWNED BY public.memberships.id;


--
-- Name: memory_requests; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.memory_requests (
    id integer NOT NULL,
    match_id integer NOT NULL,
    user_a_id integer NOT NULL,
    user_b_id integer NOT NULL,
    event_date timestamp with time zone NOT NULL,
    user_a_wants boolean,
    user_b_wants boolean,
    resolved_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: memory_requests_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.memory_requests_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: memory_requests_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.memory_requests_id_seq OWNED BY public.memory_requests.id;


--
-- Name: messages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.messages (
    id integer NOT NULL,
    sender_id integer NOT NULL,
    recipient_id integer NOT NULL,
    body text NOT NULL,
    read boolean DEFAULT false NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    order_id integer,
    type text DEFAULT 'text'::text NOT NULL,
    metadata jsonb,
    encrypted boolean DEFAULT false NOT NULL,
    ephemeral_key text,
    sender_identity_key text,
    one_time_pre_key_id integer
);


--
-- Name: messages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.messages_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: messages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.messages_id_seq OWNED BY public.messages.id;


--
-- Name: nfc_connections; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.nfc_connections (
    id integer NOT NULL,
    user_a integer NOT NULL,
    user_b integer NOT NULL,
    location text,
    confirmed_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: nfc_connections_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.nfc_connections_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: nfc_connections_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.nfc_connections_id_seq OWNED BY public.nfc_connections.id;


--
-- Name: nfc_pairing_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.nfc_pairing_tokens (
    token text NOT NULL,
    user_id integer NOT NULL,
    expires_at timestamp without time zone NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: node_applications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.node_applications (
    id integer NOT NULL,
    applicant_user_id integer NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    business_name text NOT NULL,
    address text NOT NULL,
    city text DEFAULT 'Montreal'::text NOT NULL,
    neighbourhood text,
    description text,
    instagram_handle text,
    admin_notes text,
    reviewed_at timestamp without time zone,
    business_id integer,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: node_applications_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.node_applications_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: node_applications_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.node_applications_id_seq OWNED BY public.node_applications.id;


--
-- Name: notifications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.notifications (
    id integer NOT NULL,
    user_id integer NOT NULL,
    type text NOT NULL,
    title text NOT NULL,
    body text NOT NULL,
    read boolean DEFAULT false NOT NULL,
    data jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: notifications_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.notifications_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: notifications_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.notifications_id_seq OWNED BY public.notifications.id;


--
-- Name: one_time_pre_keys; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.one_time_pre_keys (
    id integer NOT NULL,
    user_id integer NOT NULL,
    key_id integer NOT NULL,
    public_key text NOT NULL,
    used boolean DEFAULT false NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: one_time_pre_keys_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.one_time_pre_keys_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: one_time_pre_keys_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.one_time_pre_keys_id_seq OWNED BY public.one_time_pre_keys.id;


--
-- Name: order_splits; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.order_splits (
    id integer NOT NULL,
    order_id integer NOT NULL,
    split_user_id integer NOT NULL,
    split_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: order_splits_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.order_splits_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: order_splits_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.order_splits_id_seq OWNED BY public.order_splits.id;


--
-- Name: orders; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.orders (
    id integer NOT NULL,
    variety_id integer,
    location_id integer NOT NULL,
    time_slot_id integer,
    chocolate public.chocolate NOT NULL,
    finish public.finish NOT NULL,
    quantity integer NOT NULL,
    is_gift boolean DEFAULT false NOT NULL,
    total_cents integer NOT NULL,
    stripe_payment_intent_id text,
    status public.order_status DEFAULT 'pending'::public.order_status NOT NULL,
    customer_email text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    push_token text,
    nfc_token text,
    nfc_token_used boolean DEFAULT false NOT NULL,
    nfc_verified_at timestamp without time zone,
    apple_id text,
    gift_note text,
    payment_intent_id text,
    discount_applied boolean DEFAULT false NOT NULL,
    rating integer,
    rating_note text,
    worker_id integer,
    payment_method text,
    excess_amount_cents integer,
    token_id integer,
    quantity_confirmed integer,
    quantity_confirmed_at timestamp with time zone,
    delivery_address text,
    walk_in boolean DEFAULT false NOT NULL,
    batch_id integer,
    payment_captured boolean DEFAULT false NOT NULL,
    queued_at timestamp without time zone,
    menu_item_id integer
);


--
-- Name: orders_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.orders_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: orders_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.orders_id_seq OWNED BY public.orders.id;


--
-- Name: pending_connections; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.pending_connections (
    id integer NOT NULL,
    user_a_id integer NOT NULL,
    user_b_id integer NOT NULL,
    met_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone DEFAULT (now() + '48:00:00'::interval) NOT NULL,
    approved_by_a boolean DEFAULT false NOT NULL,
    approved_by_b boolean DEFAULT false NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL
);


--
-- Name: pending_connections_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.pending_connections_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: pending_connections_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.pending_connections_id_seq OWNED BY public.pending_connections.id;


--
-- Name: personal_toilets; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.personal_toilets (
    id integer NOT NULL,
    user_id integer NOT NULL,
    title text NOT NULL,
    description text,
    price_cents integer NOT NULL,
    address text NOT NULL,
    latitude numeric(10,7),
    longitude numeric(10,7),
    instagram_handle text,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: personal_toilets_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.personal_toilets_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: personal_toilets_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.personal_toilets_id_seq OWNED BY public.personal_toilets.id;


--
-- Name: personalized_menus; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.personalized_menus (
    id integer NOT NULL,
    business_id integer NOT NULL,
    user_id integer NOT NULL,
    courses jsonb NOT NULL,
    health_snapshot jsonb,
    generated_at timestamp without time zone DEFAULT now() NOT NULL,
    valid_until timestamp without time zone
);


--
-- Name: personalized_menus_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.personalized_menus_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: personalized_menus_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.personalized_menus_id_seq OWNED BY public.personalized_menus.id;


--
-- Name: platform_messages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.platform_messages (
    id integer NOT NULL,
    sender_id integer NOT NULL,
    recipient_id integer NOT NULL,
    encrypted_body text NOT NULL,
    x3dh_sender_key text,
    message_type text DEFAULT 'text'::text NOT NULL,
    fraise_object jsonb,
    sent_at timestamp with time zone DEFAULT now() NOT NULL,
    delivered_at timestamp with time zone,
    read_at timestamp with time zone,
    expires_at timestamp with time zone,
    reply_to_id integer,
    reply_to_snippet text
);


--
-- Name: platform_messages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.platform_messages_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: platform_messages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.platform_messages_id_seq OWNED BY public.platform_messages.id;


--
-- Name: popup_checkins; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.popup_checkins (
    id integer NOT NULL,
    popup_id integer NOT NULL,
    user_id integer NOT NULL,
    nfc_token text,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: popup_checkins_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.popup_checkins_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: popup_checkins_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.popup_checkins_id_seq OWNED BY public.popup_checkins.id;


--
-- Name: popup_food_orders; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.popup_food_orders (
    id integer NOT NULL,
    popup_id integer NOT NULL,
    menu_item_id integer NOT NULL,
    buyer_user_id integer NOT NULL,
    recipient_user_id integer,
    quantity integer DEFAULT 1 NOT NULL,
    total_cents integer NOT NULL,
    stripe_payment_intent_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    note text,
    claimed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: popup_food_orders_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.popup_food_orders_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: popup_food_orders_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.popup_food_orders_id_seq OWNED BY public.popup_food_orders.id;


--
-- Name: popup_merch_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.popup_merch_items (
    id integer NOT NULL,
    popup_id integer NOT NULL,
    name text NOT NULL,
    description text,
    price_cents integer NOT NULL,
    image_url text,
    sizes text[] DEFAULT '{}'::text[] NOT NULL,
    stock_remaining integer DEFAULT 0 NOT NULL,
    active boolean DEFAULT true NOT NULL,
    sort_order integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: popup_merch_items_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.popup_merch_items_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: popup_merch_items_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.popup_merch_items_id_seq OWNED BY public.popup_merch_items.id;


--
-- Name: popup_merch_orders; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.popup_merch_orders (
    id integer NOT NULL,
    popup_id integer NOT NULL,
    item_id integer NOT NULL,
    buyer_user_id integer NOT NULL,
    recipient_user_id integer,
    donated boolean DEFAULT false NOT NULL,
    size text,
    total_cents integer NOT NULL,
    stripe_payment_intent_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: popup_merch_orders_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.popup_merch_orders_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: popup_merch_orders_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.popup_merch_orders_id_seq OWNED BY public.popup_merch_orders.id;


--
-- Name: popup_nominations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.popup_nominations (
    id integer NOT NULL,
    popup_id integer NOT NULL,
    nominator_id integer NOT NULL,
    nominee_id integer NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: popup_nominations_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.popup_nominations_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: popup_nominations_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.popup_nominations_id_seq OWNED BY public.popup_nominations.id;


--
-- Name: popup_requests; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.popup_requests (
    id integer NOT NULL,
    user_id integer NOT NULL,
    venue_id integer NOT NULL,
    requested_date text NOT NULL,
    requested_time text NOT NULL,
    notes text,
    stripe_payment_intent_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: popup_requests_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.popup_requests_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: popup_requests_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.popup_requests_id_seq OWNED BY public.popup_requests.id;


--
-- Name: popup_rsvps; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.popup_rsvps (
    id integer NOT NULL,
    popup_id integer NOT NULL,
    user_id integer NOT NULL,
    stripe_payment_intent_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: popup_rsvps_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.popup_rsvps_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: popup_rsvps_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.popup_rsvps_id_seq OWNED BY public.popup_rsvps.id;


--
-- Name: portal_access; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.portal_access (
    id integer NOT NULL,
    buyer_id integer NOT NULL,
    owner_id integer NOT NULL,
    amount_cents integer NOT NULL,
    platform_cut_cents integer NOT NULL,
    source text NOT NULL,
    stripe_payment_intent_id text,
    expires_at timestamp without time zone NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: portal_access_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.portal_access_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: portal_access_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.portal_access_id_seq OWNED BY public.portal_access.id;


--
-- Name: portal_consents; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.portal_consents (
    id integer NOT NULL,
    user_id integer NOT NULL,
    consented_at timestamp without time zone DEFAULT now() NOT NULL,
    ip_address text
);


--
-- Name: portal_consents_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.portal_consents_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: portal_consents_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.portal_consents_id_seq OWNED BY public.portal_consents.id;


--
-- Name: portal_content; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.portal_content (
    id integer NOT NULL,
    user_id integer NOT NULL,
    media_url text NOT NULL,
    type text NOT NULL,
    caption text,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: portal_content_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.portal_content_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: portal_content_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.portal_content_id_seq OWNED BY public.portal_content.id;


--
-- Name: portrait_license_requests; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.portrait_license_requests (
    id integer NOT NULL,
    token_id integer NOT NULL,
    requesting_businesses jsonb NOT NULL,
    scope text DEFAULT 'in_app'::text NOT NULL,
    duration_months integer DEFAULT 3 NOT NULL,
    total_offered_cents integer NOT NULL,
    commission_cents integer DEFAULT 0 NOT NULL,
    subject_cents integer DEFAULT 0 NOT NULL,
    handle_visible boolean DEFAULT false NOT NULL,
    message text,
    status text DEFAULT 'pending'::text NOT NULL,
    expires_at timestamp without time zone NOT NULL,
    accepted_at timestamp without time zone,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: portrait_license_requests_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.portrait_license_requests_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: portrait_license_requests_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.portrait_license_requests_id_seq OWNED BY public.portrait_license_requests.id;


--
-- Name: portrait_licenses; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.portrait_licenses (
    id integer NOT NULL,
    token_id integer NOT NULL,
    request_id integer NOT NULL,
    active_from timestamp without time zone DEFAULT now() NOT NULL,
    active_until timestamp without time zone NOT NULL,
    scope text NOT NULL,
    impression_rate_cents integer DEFAULT 5 NOT NULL,
    total_impressions integer DEFAULT 0 NOT NULL,
    total_earned_cents integer DEFAULT 0 NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: portrait_licenses_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.portrait_licenses_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: portrait_licenses_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.portrait_licenses_id_seq OWNED BY public.portrait_licenses.id;


--
-- Name: portrait_token_listings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.portrait_token_listings (
    id integer NOT NULL,
    token_id integer NOT NULL,
    seller_user_id integer NOT NULL,
    asking_price_cents integer NOT NULL,
    status text DEFAULT 'listed'::text NOT NULL,
    buyer_user_id integer,
    sold_at timestamp without time zone,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: portrait_token_listings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.portrait_token_listings_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: portrait_token_listings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.portrait_token_listings_id_seq OWNED BY public.portrait_token_listings.id;


--
-- Name: portrait_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.portrait_tokens (
    id integer NOT NULL,
    nfc_uid text,
    owner_id integer NOT NULL,
    original_owner_id integer NOT NULL,
    image_url text NOT NULL,
    shot_at timestamp without time zone,
    minted_by integer NOT NULL,
    minted_at timestamp without time zone DEFAULT now() NOT NULL,
    handle_visible boolean DEFAULT false NOT NULL,
    instagram_handle text,
    open_to_licensing boolean DEFAULT true NOT NULL,
    status text DEFAULT 'active'::text NOT NULL
);


--
-- Name: portrait_tokens_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.portrait_tokens_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: portrait_tokens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.portrait_tokens_id_seq OWNED BY public.portrait_tokens.id;


--
-- Name: portraits; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.portraits (
    id integer NOT NULL,
    business_id integer NOT NULL,
    image_url text NOT NULL,
    subject_name text,
    season text,
    campaign_title text,
    sort_order integer DEFAULT 0 NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: portraits_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.portraits_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: portraits_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.portraits_id_seq OWNED BY public.portraits.id;


--
-- Name: preorders; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.preorders (
    id integer NOT NULL,
    user_id integer NOT NULL,
    variety_id integer,
    variety_name_requested text,
    quantity integer DEFAULT 1 NOT NULL,
    notes text,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    fulfilled_at timestamp with time zone
);


--
-- Name: preorders_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.preorders_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: preorders_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.preorders_id_seq OWNED BY public.preorders.id;


--
-- Name: product_bundles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.product_bundles (
    id integer NOT NULL,
    name text NOT NULL,
    description text,
    price_cents integer NOT NULL,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: product_bundles_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.product_bundles_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: product_bundles_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.product_bundles_id_seq OWNED BY public.product_bundles.id;


--
-- Name: promotion_deliveries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.promotion_deliveries (
    id integer NOT NULL,
    promotion_id integer NOT NULL,
    user_id integer NOT NULL,
    delivered_at timestamp with time zone DEFAULT now() NOT NULL,
    read_at timestamp with time zone,
    fee_cents integer DEFAULT 200 NOT NULL
);


--
-- Name: promotion_deliveries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.promotion_deliveries_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: promotion_deliveries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.promotion_deliveries_id_seq OWNED BY public.promotion_deliveries.id;


--
-- Name: provenance_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.provenance_tokens (
    id integer NOT NULL,
    greenhouse_id integer,
    location_id integer,
    provenance_ledger text DEFAULT '[]'::text NOT NULL,
    nfc_token text,
    minted_at timestamp without time zone DEFAULT now() NOT NULL,
    greenhouse_name text NOT NULL,
    greenhouse_location text NOT NULL
);


--
-- Name: provenance_tokens_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.provenance_tokens_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: provenance_tokens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.provenance_tokens_id_seq OWNED BY public.provenance_tokens.id;


--
-- Name: referral_codes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.referral_codes (
    id integer NOT NULL,
    user_id integer NOT NULL,
    code text NOT NULL,
    uses integer DEFAULT 0 NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: referral_codes_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.referral_codes_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: referral_codes_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.referral_codes_id_seq OWNED BY public.referral_codes.id;


--
-- Name: referrals; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.referrals (
    id integer NOT NULL,
    referrer_user_id integer NOT NULL,
    referee_user_id integer NOT NULL,
    code text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    completed_at timestamp with time zone,
    reward_granted_at timestamp with time zone
);


--
-- Name: referrals_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.referrals_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: referrals_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.referrals_id_seq OWNED BY public.referrals.id;


--
-- Name: reservation_bookings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.reservation_bookings (
    id integer NOT NULL,
    offer_id integer NOT NULL,
    initiator_user_id integer NOT NULL,
    guest_user_id integer,
    status text DEFAULT 'seeking_pair'::text NOT NULL,
    invite_expires_at timestamp without time zone,
    confirmed_at timestamp without time zone,
    strawberry_order_placed boolean DEFAULT false NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: reservation_bookings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.reservation_bookings_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: reservation_bookings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.reservation_bookings_id_seq OWNED BY public.reservation_bookings.id;


--
-- Name: reservation_offers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.reservation_offers (
    id integer NOT NULL,
    business_id integer NOT NULL,
    title text NOT NULL,
    description text,
    mode text DEFAULT 'platform_match'::text NOT NULL,
    value_cents integer NOT NULL,
    commission_cents integer DEFAULT 0 NOT NULL,
    drink_description text,
    reservation_date text,
    reservation_time text,
    slots_total integer DEFAULT 1 NOT NULL,
    slots_remaining integer DEFAULT 1 NOT NULL,
    status text DEFAULT 'active'::text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: reservation_offers_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.reservation_offers_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: reservation_offers_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.reservation_offers_id_seq OWNED BY public.reservation_offers.id;


--
-- Name: season_patronages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.season_patronages (
    id integer NOT NULL,
    location_id integer NOT NULL,
    season_year integer NOT NULL,
    price_per_year_cents integer NOT NULL,
    years_claimed integer,
    patron_user_id integer,
    platform_cut_cents integer DEFAULT 0 NOT NULL,
    status text DEFAULT 'available'::text NOT NULL,
    stripe_payment_intent_id text,
    claimed_at timestamp without time zone,
    requested_by integer,
    approved_by_admin boolean DEFAULT false NOT NULL,
    location_name text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: season_patronages_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.season_patronages_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: season_patronages_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.season_patronages_id_seq OWNED BY public.season_patronages.id;


--
-- Name: staff_sessions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.staff_sessions (
    id integer NOT NULL,
    staff_user_id integer NOT NULL,
    session_date date DEFAULT CURRENT_DATE NOT NULL,
    orders_processed integer DEFAULT 0,
    avg_prep_seconds integer,
    accuracy_pct numeric(5,2)
);


--
-- Name: staff_sessions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.staff_sessions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: staff_sessions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.staff_sessions_id_seq OWNED BY public.staff_sessions.id;


--
-- Name: standing_order_tiers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.standing_order_tiers (
    id integer NOT NULL,
    name text NOT NULL,
    description text,
    quantity_per_delivery integer DEFAULT 1 NOT NULL,
    price_cents integer NOT NULL,
    active boolean DEFAULT true NOT NULL,
    sort_order integer DEFAULT 0 NOT NULL
);


--
-- Name: standing_order_tiers_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.standing_order_tiers_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: standing_order_tiers_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.standing_order_tiers_id_seq OWNED BY public.standing_order_tiers.id;


--
-- Name: standing_order_transfers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.standing_order_transfers (
    id integer NOT NULL,
    standing_order_id integer NOT NULL,
    from_user_id integer NOT NULL,
    to_user_id integer,
    to_user_code text,
    initiated_at timestamp with time zone DEFAULT now() NOT NULL,
    accepted_at timestamp with time zone,
    cancelled_at timestamp with time zone,
    status text DEFAULT 'pending'::text NOT NULL
);


--
-- Name: standing_order_transfers_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.standing_order_transfers_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: standing_order_transfers_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.standing_order_transfers_id_seq OWNED BY public.standing_order_transfers.id;


--
-- Name: standing_order_waitlist; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.standing_order_waitlist (
    id integer NOT NULL,
    user_id integer NOT NULL,
    referred_by_user_id integer,
    joined_at timestamp with time zone DEFAULT now() NOT NULL,
    notified_at timestamp with time zone,
    claim_expires_at timestamp with time zone,
    claimed_at timestamp with time zone,
    status text DEFAULT 'waiting'::text NOT NULL
);


--
-- Name: standing_order_waitlist_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.standing_order_waitlist_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: standing_order_waitlist_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.standing_order_waitlist_id_seq OWNED BY public.standing_order_waitlist.id;


--
-- Name: standing_orders; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.standing_orders (
    id integer NOT NULL,
    sender_id integer NOT NULL,
    recipient_id integer,
    variety_id integer NOT NULL,
    chocolate public.chocolate NOT NULL,
    finish public.finish NOT NULL,
    quantity integer NOT NULL,
    location_id integer NOT NULL,
    time_slot_preference text NOT NULL,
    frequency public.standing_order_frequency NOT NULL,
    next_order_date timestamp without time zone NOT NULL,
    stripe_subscription_id text,
    gift_tone public.gift_tone,
    status public.standing_order_status DEFAULT 'active'::public.standing_order_status NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    gift_message text,
    renewal_notified_30_at timestamp with time zone,
    expires_at timestamp with time zone,
    recipient_email text,
    tier text DEFAULT 'standard'::text,
    gifted_by_user_id integer,
    renewal_notified_60_at timestamp with time zone,
    is_gift_standing_order boolean DEFAULT false NOT NULL
);


--
-- Name: standing_orders_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.standing_orders_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: standing_orders_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.standing_orders_id_seq OWNED BY public.standing_orders.id;


--
-- Name: table_booking_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.table_booking_tokens (
    id integer NOT NULL,
    token text NOT NULL,
    booking_id integer NOT NULL,
    action text NOT NULL,
    used boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT table_booking_tokens_action_check CHECK ((action = ANY (ARRAY['confirm'::text, 'refund'::text])))
);


--
-- Name: table_booking_tokens_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.table_booking_tokens_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: table_booking_tokens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.table_booking_tokens_id_seq OWNED BY public.table_booking_tokens.id;


--
-- Name: table_bookings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.table_bookings (
    id integer NOT NULL,
    event_id integer NOT NULL,
    name text NOT NULL,
    email text NOT NULL,
    seats integer DEFAULT 1 NOT NULL,
    total_cents integer NOT NULL,
    stripe_payment_intent_id text,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: table_bookings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.table_bookings_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: table_bookings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.table_bookings_id_seq OWNED BY public.table_bookings.id;


--
-- Name: table_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.table_events (
    id integer NOT NULL,
    instructor_id integer,
    title text NOT NULL,
    venue_name text NOT NULL,
    venue_address text,
    event_date timestamp without time zone,
    duration_minutes integer DEFAULT 120 NOT NULL,
    price_cents integer NOT NULL,
    capacity integer DEFAULT 12 NOT NULL,
    seats_taken integer DEFAULT 0 NOT NULL,
    description text,
    stripe_price_id text,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    date_tbd boolean DEFAULT true NOT NULL,
    parent_event_id integer,
    venue_slug text,
    event_type text DEFAULT 'group'::text NOT NULL,
    threshold integer
);


--
-- Name: table_events_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.table_events_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: table_events_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.table_events_id_seq OWNED BY public.table_events.id;


--
-- Name: table_instructors; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.table_instructors (
    id integer NOT NULL,
    name text NOT NULL,
    bio text,
    photo_url text,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: table_instructors_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.table_instructors_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: table_instructors_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.table_instructors_id_seq OWNED BY public.table_instructors.id;


--
-- Name: table_memberships; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.table_memberships (
    id integer NOT NULL,
    slug text NOT NULL,
    name text NOT NULL,
    email text NOT NULL,
    amount_cents integer NOT NULL,
    stripe_payment_intent_id text,
    status text DEFAULT 'waiting'::text NOT NULL,
    events_attended integer DEFAULT 0 NOT NULL,
    last_called_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    confirm_token text,
    confirmed_at timestamp with time zone,
    refunded_at timestamp with time zone
);


--
-- Name: table_memberships_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.table_memberships_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: table_memberships_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.table_memberships_id_seq OWNED BY public.table_memberships.id;


--
-- Name: table_venue_sessions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.table_venue_sessions (
    id integer NOT NULL,
    slug text NOT NULL,
    token text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: table_venue_sessions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.table_venue_sessions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: table_venue_sessions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.table_venue_sessions_id_seq OWNED BY public.table_venue_sessions.id;


--
-- Name: table_venues; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.table_venues (
    id integer NOT NULL,
    slug text NOT NULL,
    display_name text NOT NULL,
    email text NOT NULL,
    password_hash text NOT NULL,
    price_cents integer DEFAULT 12000 NOT NULL,
    active boolean DEFAULT true NOT NULL,
    stripe_connect_account_id text,
    stripe_connect_onboarded boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: table_venues_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.table_venues_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: table_venues_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.table_venues_id_seq OWNED BY public.table_venues.id;


--
-- Name: tasting_feed_reactions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.tasting_feed_reactions (
    id integer NOT NULL,
    entry_id integer NOT NULL,
    user_id integer NOT NULL,
    emoji text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: tasting_feed_reactions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.tasting_feed_reactions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasting_feed_reactions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasting_feed_reactions_id_seq OWNED BY public.tasting_feed_reactions.id;


--
-- Name: tasting_journal; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.tasting_journal (
    id integer NOT NULL,
    user_id integer NOT NULL,
    variety_id integer NOT NULL,
    rating integer NOT NULL,
    notes text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    public boolean DEFAULT false NOT NULL,
    CONSTRAINT tasting_journal_rating_check CHECK (((rating >= 1) AND (rating <= 5)))
);


--
-- Name: tasting_journal_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.tasting_journal_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tasting_journal_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.tasting_journal_id_seq OWNED BY public.tasting_journal.id;


--
-- Name: time_slots; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.time_slots (
    id integer NOT NULL,
    location_id integer NOT NULL,
    date date NOT NULL,
    "time" text NOT NULL,
    capacity integer NOT NULL,
    booked integer DEFAULT 0 NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: time_slots_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.time_slots_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: time_slots_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.time_slots_id_seq OWNED BY public.time_slots.id;


--
-- Name: toilet_visits; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.toilet_visits (
    id integer NOT NULL,
    user_id integer NOT NULL,
    business_id integer,
    fee_cents integer NOT NULL,
    payment_method text NOT NULL,
    stripe_payment_intent_id text,
    paid boolean DEFAULT false NOT NULL,
    access_code text,
    rating integer,
    review_note text,
    reviewed_at timestamp without time zone,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    access_code_expires_at timestamp without time zone,
    personal_toilet_id integer
);


--
-- Name: toilet_visits_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.toilet_visits_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: toilet_visits_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.toilet_visits_id_seq OWNED BY public.toilet_visits.id;


--
-- Name: typing_indicators; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.typing_indicators (
    user_id integer NOT NULL,
    contact_id integer NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: user_business_visits; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_business_visits (
    id integer NOT NULL,
    user_id integer NOT NULL,
    business_id integer NOT NULL,
    beacon_uuid text NOT NULL,
    visited_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: user_business_visits_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_business_visits_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_business_visits_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_business_visits_id_seq OWNED BY public.user_business_visits.id;


--
-- Name: user_challenge_progress; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_challenge_progress (
    id integer NOT NULL,
    user_id integer NOT NULL,
    challenge_id integer NOT NULL,
    progress integer DEFAULT 0,
    completed_at timestamp with time zone
);


--
-- Name: user_challenge_progress_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_challenge_progress_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_challenge_progress_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_challenge_progress_id_seq OWNED BY public.user_challenge_progress.id;


--
-- Name: user_earnings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_earnings (
    id integer NOT NULL,
    user_id integer NOT NULL,
    source_type text NOT NULL,
    source_id integer NOT NULL,
    amount_cents integer NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: user_earnings_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_earnings_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_earnings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_earnings_id_seq OWNED BY public.user_earnings.id;


--
-- Name: user_follows; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_follows (
    id integer NOT NULL,
    follower_id integer NOT NULL,
    followee_id integer NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: user_follows_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_follows_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_follows_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_follows_id_seq OWNED BY public.user_follows.id;


--
-- Name: user_keys; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_keys (
    id integer NOT NULL,
    user_id integer NOT NULL,
    identity_key text NOT NULL,
    signed_pre_key text NOT NULL,
    signed_pre_key_sig text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    updated_at timestamp without time zone DEFAULT now() NOT NULL,
    identity_signing_key text
);


--
-- Name: user_keys_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_keys_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_keys_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_keys_id_seq OWNED BY public.user_keys.id;


--
-- Name: user_map_entries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_map_entries (
    id integer NOT NULL,
    map_id integer NOT NULL,
    business_id integer NOT NULL,
    note text,
    sort_order integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: user_map_entries_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_map_entries_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_map_entries_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_map_entries_id_seq OWNED BY public.user_map_entries.id;


--
-- Name: user_maps; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_maps (
    id integer NOT NULL,
    user_id integer NOT NULL,
    name text NOT NULL,
    description text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: user_maps_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_maps_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_maps_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_maps_id_seq OWNED BY public.user_maps.id;


--
-- Name: user_saves; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_saves (
    id integer NOT NULL,
    saver_id integer NOT NULL,
    saved_user_id integer NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: user_saves_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.user_saves_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_saves_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.user_saves_id_seq OWNED BY public.user_saves.id;


--
-- Name: users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.users (
    id integer NOT NULL,
    email text NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    referred_by_code text,
    stripe_customer_id text,
    portal_opted_in boolean DEFAULT false NOT NULL,
    portrait_url text,
    worker_status text,
    banned boolean DEFAULT false NOT NULL,
    ban_reason text,
    display_name text,
    notification_prefs jsonb,
    verified_at timestamp without time zone,
    verified_by text,
    photographed boolean DEFAULT false NOT NULL,
    campaign_interest boolean DEFAULT false NOT NULL,
    verified boolean DEFAULT false NOT NULL,
    apple_user_id text,
    push_token text,
    is_dj boolean DEFAULT false NOT NULL,
    user_code text,
    fraise_chat_email text,
    is_shop boolean DEFAULT false NOT NULL,
    business_id integer,
    identity_verified boolean DEFAULT false NOT NULL,
    identity_verified_at timestamp with time zone,
    identity_session_id text,
    id_attested_by integer,
    id_attested_at timestamp with time zone,
    id_attestation_expires_at timestamp with time zone,
    id_verified_name text,
    id_verified_dob text,
    identity_verified_expires_at timestamp with time zone,
    verification_renewal_due_at timestamp with time zone,
    is_dorotka boolean DEFAULT false NOT NULL,
    stripe_connect_account_id text,
    stripe_connect_onboarded boolean DEFAULT false NOT NULL,
    ad_balance_cents integer DEFAULT 0 NOT NULL,
    social_time_bank_seconds integer DEFAULT 0 NOT NULL,
    social_time_bank_updated_at timestamp with time zone,
    social_lifetime_credits_seconds integer DEFAULT 0 NOT NULL,
    current_streak_weeks integer DEFAULT 0 NOT NULL,
    longest_streak_weeks integer DEFAULT 0 NOT NULL,
    last_tap_week text,
    eth_address text,
    platform_credit_cents integer DEFAULT 0 NOT NULL,
    feed_visible boolean DEFAULT false NOT NULL,
    password_hash text,
    table_verified boolean DEFAULT false NOT NULL,
    reset_token text,
    reset_token_expires_at timestamp with time zone,
    status text,
    open_to_dates boolean DEFAULT false NOT NULL
);


--
-- Name: users_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.users_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: users_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.users_id_seq OWNED BY public.users.id;


--
-- Name: varieties; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.varieties (
    id integer NOT NULL,
    name text NOT NULL,
    description text,
    source_farm text,
    source_location text,
    price_cents integer NOT NULL,
    stock_remaining integer DEFAULT 0 NOT NULL,
    harvest_date date,
    tag text,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    location_id integer,
    image_url text,
    sort_order integer DEFAULT 0 NOT NULL,
    variety_type text DEFAULT 'strawberry'::text NOT NULL,
    time_credits_days integer DEFAULT 30 NOT NULL,
    social_tier public.social_tier
);


--
-- Name: varieties_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.varieties_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: varieties_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.varieties_id_seq OWNED BY public.varieties.id;


--
-- Name: variety_drops; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.variety_drops (
    id integer NOT NULL,
    variety_id integer NOT NULL,
    name text,
    description text,
    drops_at timestamp with time zone NOT NULL,
    quantity integer NOT NULL,
    per_user_limit integer DEFAULT 1 NOT NULL,
    requires_standing_order boolean DEFAULT true NOT NULL,
    price_cents integer NOT NULL,
    status text DEFAULT 'scheduled'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: variety_drops_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.variety_drops_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: variety_drops_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.variety_drops_id_seq OWNED BY public.variety_drops.id;


--
-- Name: variety_profiles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.variety_profiles (
    id integer NOT NULL,
    variety_id integer NOT NULL,
    sweetness numeric(4,1) DEFAULT 5 NOT NULL,
    acidity numeric(4,1) DEFAULT 5 NOT NULL,
    aroma numeric(4,1) DEFAULT 5 NOT NULL,
    texture numeric(4,1) DEFAULT 5 NOT NULL,
    intensity numeric(4,1) DEFAULT 5 NOT NULL,
    pairing_chocolate text,
    pairing_finish text,
    farm_distance_km numeric(7,1),
    tasting_notes text,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    brix_score numeric(4,1),
    growing_method text,
    farm_lng numeric(9,6),
    altitude_m integer,
    co2_grams integer,
    soil_type text,
    folate_mcg numeric(8,2),
    orac_value integer,
    farm_milestones_json text,
    carbon_offset_program text,
    eat_by_days integer,
    manganese_mg numeric(8,3),
    fermentation_profile_json text,
    irrigation_method text,
    sunlight_hours numeric(5,1),
    moon_phase_at_harvest text,
    recipe_name text,
    potassium_mg numeric(8,1),
    hue_value integer,
    cover_crop text,
    price_history_json text,
    parent_a text,
    recipe_description text,
    vitamin_k_mcg numeric(8,2),
    farmer_name text,
    terrain_type text,
    farm_id integer,
    parent_b text,
    harvest_weather_json text,
    farmer_quote text,
    prevailing_wind text,
    farm_webcam_url text,
    farm_photo_url text,
    certifications_json text,
    ambient_audio_url text,
    farm_lat numeric(10,6),
    producer_video_url text,
    farm_founded_year integer,
    mascot_id text
);


--
-- Name: variety_profiles_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.variety_profiles_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: variety_profiles_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.variety_profiles_id_seq OWNED BY public.variety_profiles.id;


--
-- Name: variety_reviews; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.variety_reviews (
    id integer NOT NULL,
    user_id integer NOT NULL,
    variety_id integer NOT NULL,
    rating integer NOT NULL,
    note text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT variety_reviews_rating_check CHECK (((rating >= 1) AND (rating <= 5)))
);


--
-- Name: variety_reviews_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.variety_reviews_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: variety_reviews_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.variety_reviews_id_seq OWNED BY public.variety_reviews.id;


--
-- Name: variety_seasons; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.variety_seasons (
    id integer NOT NULL,
    variety_id integer NOT NULL,
    available_from date NOT NULL,
    available_until date NOT NULL,
    year integer DEFAULT (EXTRACT(year FROM now()))::integer NOT NULL,
    notes text
);


--
-- Name: variety_seasons_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.variety_seasons_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: variety_seasons_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.variety_seasons_id_seq OWNED BY public.variety_seasons.id;


--
-- Name: verification_payments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.verification_payments (
    id integer NOT NULL,
    user_id integer NOT NULL,
    type text NOT NULL,
    amount_cents integer NOT NULL,
    stripe_payment_intent_id text,
    stripe_client_secret text,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: verification_payments_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.verification_payments_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: verification_payments_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.verification_payments_id_seq OWNED BY public.verification_payments.id;


--
-- Name: walk_in_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.walk_in_tokens (
    id integer NOT NULL,
    token text NOT NULL,
    location_id integer NOT NULL,
    variety_id integer NOT NULL,
    claimed boolean DEFAULT false NOT NULL,
    claimed_order_id integer,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: walk_in_tokens_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.walk_in_tokens_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: walk_in_tokens_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.walk_in_tokens_id_seq OWNED BY public.walk_in_tokens.id;


--
-- Name: webhook_subscriptions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.webhook_subscriptions (
    id integer NOT NULL,
    user_id integer NOT NULL,
    url text NOT NULL,
    events text[] DEFAULT '{}'::text[] NOT NULL,
    secret text NOT NULL,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_fired_at timestamp with time zone,
    last_status_code integer
);


--
-- Name: webhook_subscriptions_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.webhook_subscriptions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: webhook_subscriptions_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.webhook_subscriptions_id_seq OWNED BY public.webhook_subscriptions.id;


--


--
-- Name: ad_campaigns id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ad_campaigns ALTER COLUMN id SET DEFAULT nextval('public.ad_campaigns_id_seq'::regclass);


--
-- Name: ad_impressions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ad_impressions ALTER COLUMN id SET DEFAULT nextval('public.ad_impressions_id_seq'::regclass);


--
-- Name: akene_events id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.akene_events ALTER COLUMN id SET DEFAULT nextval('public.akene_events_id_seq'::regclass);


--
-- Name: akene_invitations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.akene_invitations ALTER COLUMN id SET DEFAULT nextval('public.akene_invitations_id_seq'::regclass);


--
-- Name: akene_purchases id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.akene_purchases ALTER COLUMN id SET DEFAULT nextval('public.akene_purchases_id_seq'::regclass);


--
-- Name: ar_notes id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ar_notes ALTER COLUMN id SET DEFAULT nextval('public.ar_notes_id_seq'::regclass);


--
-- Name: art_acquisitions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_acquisitions ALTER COLUMN id SET DEFAULT nextval('public.art_acquisitions_id_seq'::regclass);


--
-- Name: art_auctions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_auctions ALTER COLUMN id SET DEFAULT nextval('public.art_auctions_id_seq'::regclass);


--
-- Name: art_bids id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_bids ALTER COLUMN id SET DEFAULT nextval('public.art_bids_id_seq'::regclass);


--
-- Name: art_management_fees id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_management_fees ALTER COLUMN id SET DEFAULT nextval('public.art_management_fees_id_seq'::regclass);


--
-- Name: art_pitches id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_pitches ALTER COLUMN id SET DEFAULT nextval('public.art_pitches_id_seq'::regclass);


--
-- Name: artworks id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.artworks ALTER COLUMN id SET DEFAULT nextval('public.artworks_id_seq'::regclass);


--
-- Name: batch_preferences id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_preferences ALTER COLUMN id SET DEFAULT nextval('public.batch_preferences_id_seq'::regclass);


--
-- Name: batches id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batches ALTER COLUMN id SET DEFAULT nextval('public.batches_id_seq'::regclass);


--
-- Name: beacons id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.beacons ALTER COLUMN id SET DEFAULT nextval('public.beacons_id_seq'::regclass);


--
-- Name: bundle_orders id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_orders ALTER COLUMN id SET DEFAULT nextval('public.bundle_orders_id_seq'::regclass);


--
-- Name: bundle_varieties id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_varieties ALTER COLUMN id SET DEFAULT nextval('public.bundle_varieties_id_seq'::regclass);


--
-- Name: business_accounts id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_accounts ALTER COLUMN id SET DEFAULT nextval('public.business_accounts_id_seq'::regclass);


--
-- Name: business_menu_items id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_menu_items ALTER COLUMN id SET DEFAULT nextval('public.business_menu_items_id_seq'::regclass);


--
-- Name: business_promotions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_promotions ALTER COLUMN id SET DEFAULT nextval('public.business_promotions_id_seq'::regclass);


--
-- Name: business_proposals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_proposals ALTER COLUMN id SET DEFAULT nextval('public.business_proposals_id_seq'::regclass);


--
-- Name: business_visits id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_visits ALTER COLUMN id SET DEFAULT nextval('public.business_visits_id_seq'::regclass);


--
-- Name: businesses id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.businesses ALTER COLUMN id SET DEFAULT nextval('public.businesses_id_seq'::regclass);


--
-- Name: campaign_commissions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.campaign_commissions ALTER COLUMN id SET DEFAULT nextval('public.campaign_commissions_id_seq'::regclass);


--
-- Name: campaign_signups id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.campaign_signups ALTER COLUMN id SET DEFAULT nextval('public.campaign_signups_id_seq'::regclass);


--
-- Name: campaigns id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.campaigns ALTER COLUMN id SET DEFAULT nextval('public.campaigns_id_seq'::regclass);


--
-- Name: co_scans id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.co_scans ALTER COLUMN id SET DEFAULT nextval('public.co_scans_id_seq'::regclass);


--
-- Name: collectif_challenges id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collectif_challenges ALTER COLUMN id SET DEFAULT nextval('public.collectif_challenges_id_seq'::regclass);


--
-- Name: collectif_commitments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collectif_commitments ALTER COLUMN id SET DEFAULT nextval('public.collectif_commitments_id_seq'::regclass);


--
-- Name: collectifs id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collectifs ALTER COLUMN id SET DEFAULT nextval('public.collectifs_id_seq'::regclass);


--
-- Name: community_events id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.community_events ALTER COLUMN id SET DEFAULT nextval('public.community_events_id_seq'::regclass);


--
-- Name: community_fund id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.community_fund ALTER COLUMN id SET DEFAULT nextval('public.community_fund_id_seq'::regclass);


--
-- Name: community_fund_contributions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.community_fund_contributions ALTER COLUMN id SET DEFAULT nextval('public.community_fund_contributions_id_seq'::regclass);


--
-- Name: community_popup_interest id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.community_popup_interest ALTER COLUMN id SET DEFAULT nextval('public.community_popup_interest_id_seq'::regclass);


--
-- Name: connections id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.connections ALTER COLUMN id SET DEFAULT nextval('public.connections_id_seq'::regclass);


--
-- Name: contract_requests id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contract_requests ALTER COLUMN id SET DEFAULT nextval('public.contract_requests_id_seq'::regclass);


--
-- Name: conversation_archives id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.conversation_archives ALTER COLUMN id SET DEFAULT nextval('public.conversation_archives_id_seq'::regclass);


--
-- Name: corporate_accounts id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.corporate_accounts ALTER COLUMN id SET DEFAULT nextval('public.corporate_accounts_id_seq'::regclass);


--
-- Name: corporate_members id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.corporate_members ALTER COLUMN id SET DEFAULT nextval('public.corporate_members_id_seq'::regclass);


--
-- Name: credit_transactions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.credit_transactions ALTER COLUMN id SET DEFAULT nextval('public.credit_transactions_id_seq'::regclass);


--
-- Name: date_invitations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_invitations ALTER COLUMN id SET DEFAULT nextval('public.date_invitations_id_seq'::regclass);


--
-- Name: date_matches id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_matches ALTER COLUMN id SET DEFAULT nextval('public.date_matches_id_seq'::regclass);


--
-- Name: date_offers id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_offers ALTER COLUMN id SET DEFAULT nextval('public.date_offers_id_seq'::regclass);


--
-- Name: device_attestations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_attestations ALTER COLUMN id SET DEFAULT nextval('public.device_attestations_id_seq'::regclass);


--
-- Name: device_pairing_tokens id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_pairing_tokens ALTER COLUMN id SET DEFAULT nextval('public.device_pairing_tokens_id_seq'::regclass);


--
-- Name: devices id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.devices ALTER COLUMN id SET DEFAULT nextval('public.devices_id_seq'::regclass);


--
-- Name: dj_offers id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.dj_offers ALTER COLUMN id SET DEFAULT nextval('public.dj_offers_id_seq'::regclass);


--
-- Name: drop_claims id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.drop_claims ALTER COLUMN id SET DEFAULT nextval('public.drop_claims_id_seq'::regclass);


--
-- Name: drop_waitlist id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.drop_waitlist ALTER COLUMN id SET DEFAULT nextval('public.drop_waitlist_id_seq'::regclass);


--
-- Name: drops id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.drops ALTER COLUMN id SET DEFAULT nextval('public.drops_id_seq'::regclass);


--
-- Name: editorial_pieces id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.editorial_pieces ALTER COLUMN id SET DEFAULT nextval('public.editorial_pieces_id_seq'::regclass);


--
-- Name: employment_contracts id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.employment_contracts ALTER COLUMN id SET DEFAULT nextval('public.employment_contracts_id_seq'::regclass);


--
-- Name: evening_tokens id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.evening_tokens ALTER COLUMN id SET DEFAULT nextval('public.evening_tokens_id_seq'::regclass);


--
-- Name: explicit_portals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.explicit_portals ALTER COLUMN id SET DEFAULT nextval('public.explicit_portals_id_seq'::regclass);


--
-- Name: farm_visit_bookings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.farm_visit_bookings ALTER COLUMN id SET DEFAULT nextval('public.farm_visit_bookings_id_seq'::regclass);


--
-- Name: farm_visits id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.farm_visits ALTER COLUMN id SET DEFAULT nextval('public.farm_visits_id_seq'::regclass);


--
-- Name: fraise_business_sessions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_business_sessions ALTER COLUMN id SET DEFAULT nextval('public.fraise_business_sessions_id_seq'::regclass);


--
-- Name: fraise_businesses id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_businesses ALTER COLUMN id SET DEFAULT nextval('public.fraise_businesses_id_seq'::regclass);


--
-- Name: fraise_claims id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_claims ALTER COLUMN id SET DEFAULT nextval('public.fraise_claims_id_seq'::regclass);


--
-- Name: fraise_credit_purchases id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_credit_purchases ALTER COLUMN id SET DEFAULT nextval('public.fraise_credit_purchases_id_seq'::regclass);


--
-- Name: fraise_events id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_events ALTER COLUMN id SET DEFAULT nextval('public.fraise_events_id_seq'::regclass);


--
-- Name: fraise_interest id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_interest ALTER COLUMN id SET DEFAULT nextval('public.fraise_interest_id_seq'::regclass);


--
-- Name: fraise_invitations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_invitations ALTER COLUMN id SET DEFAULT nextval('public.fraise_invitations_id_seq'::regclass);


--
-- Name: fraise_member_resets id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_member_resets ALTER COLUMN id SET DEFAULT nextval('public.fraise_member_resets_id_seq'::regclass);


--
-- Name: fraise_member_sessions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_member_sessions ALTER COLUMN id SET DEFAULT nextval('public.fraise_member_sessions_id_seq'::regclass);


--
-- Name: fraise_members id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_members ALTER COLUMN id SET DEFAULT nextval('public.fraise_members_id_seq'::regclass);


--
-- Name: fraise_messages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_messages ALTER COLUMN id SET DEFAULT nextval('public.fraise_messages_id_seq'::regclass);


--
-- Name: fund_contributions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fund_contributions ALTER COLUMN id SET DEFAULT nextval('public.fund_contributions_id_seq'::regclass);


--
-- Name: gift_registry id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gift_registry ALTER COLUMN id SET DEFAULT nextval('public.gift_registry_id_seq'::regclass);


--
-- Name: gifts id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gifts ALTER COLUMN id SET DEFAULT nextval('public.gifts_id_seq'::regclass);


--
-- Name: greenhouse_funding id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.greenhouse_funding ALTER COLUMN id SET DEFAULT nextval('public.greenhouse_funding_id_seq'::regclass);


--
-- Name: greenhouses id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.greenhouses ALTER COLUMN id SET DEFAULT nextval('public.greenhouses_id_seq'::regclass);


--
-- Name: harvest_logs id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.harvest_logs ALTER COLUMN id SET DEFAULT nextval('public.harvest_logs_id_seq'::regclass);


--
-- Name: health_profiles id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.health_profiles ALTER COLUMN id SET DEFAULT nextval('public.health_profiles_id_seq'::regclass);


--
-- Name: id_attestation_log id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.id_attestation_log ALTER COLUMN id SET DEFAULT nextval('public.id_attestation_log_id_seq'::regclass);


--
-- Name: itineraries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itineraries ALTER COLUMN id SET DEFAULT nextval('public.itineraries_id_seq'::regclass);


--
-- Name: itinerary_destinations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itinerary_destinations ALTER COLUMN id SET DEFAULT nextval('public.itinerary_destinations_id_seq'::regclass);


--
-- Name: itinerary_proposals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itinerary_proposals ALTER COLUMN id SET DEFAULT nextval('public.itinerary_proposals_id_seq'::regclass);


--
-- Name: job_applications id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_applications ALTER COLUMN id SET DEFAULT nextval('public.job_applications_id_seq'::regclass);


--
-- Name: job_interviews id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_interviews ALTER COLUMN id SET DEFAULT nextval('public.job_interviews_id_seq'::regclass);


--
-- Name: job_ledger_entries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_ledger_entries ALTER COLUMN id SET DEFAULT nextval('public.job_ledger_entries_id_seq'::regclass);


--
-- Name: job_postings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_postings ALTER COLUMN id SET DEFAULT nextval('public.job_postings_id_seq'::regclass);


--
-- Name: key_challenges id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.key_challenges ALTER COLUMN id SET DEFAULT nextval('public.key_challenges_id_seq'::regclass);


--
-- Name: kommune_assignments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.kommune_assignments ALTER COLUMN id SET DEFAULT nextval('public.kommune_assignments_id_seq'::regclass);


--
-- Name: kommune_flavour_suggestions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.kommune_flavour_suggestions ALTER COLUMN id SET DEFAULT nextval('public.kommune_flavour_suggestions_id_seq'::regclass);


--
-- Name: kommune_press_applications id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.kommune_press_applications ALTER COLUMN id SET DEFAULT nextval('public.kommune_press_applications_id_seq'::regclass);


--
-- Name: kommune_ratings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.kommune_ratings ALTER COLUMN id SET DEFAULT nextval('public.kommune_ratings_id_seq'::regclass);


--
-- Name: kommune_reservations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.kommune_reservations ALTER COLUMN id SET DEFAULT nextval('public.kommune_reservations_id_seq'::regclass);


--
-- Name: legitimacy_events id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.legitimacy_events ALTER COLUMN id SET DEFAULT nextval('public.legitimacy_events_id_seq'::regclass);


--
-- Name: location_funding id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.location_funding ALTER COLUMN id SET DEFAULT nextval('public.location_funding_id_seq'::regclass);


--
-- Name: location_staff id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.location_staff ALTER COLUMN id SET DEFAULT nextval('public.location_staff_id_seq'::regclass);


--
-- Name: locations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.locations ALTER COLUMN id SET DEFAULT nextval('public.locations_id_seq'::regclass);


--
-- Name: market_dates id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_dates ALTER COLUMN id SET DEFAULT nextval('public.market_dates_id_seq'::regclass);


--
-- Name: market_listings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_listings ALTER COLUMN id SET DEFAULT nextval('public.market_listings_id_seq'::regclass);


--
-- Name: market_order_items id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_order_items ALTER COLUMN id SET DEFAULT nextval('public.market_order_items_id_seq'::regclass);


--
-- Name: market_orders id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_orders ALTER COLUMN id SET DEFAULT nextval('public.market_orders_id_seq'::regclass);


--
-- Name: market_orders_v2 id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_orders_v2 ALTER COLUMN id SET DEFAULT nextval('public.market_orders_v2_id_seq'::regclass);


--
-- Name: market_products id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_products ALTER COLUMN id SET DEFAULT nextval('public.market_products_id_seq'::regclass);


--
-- Name: market_stalls id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_stalls ALTER COLUMN id SET DEFAULT nextval('public.market_stalls_id_seq'::regclass);


--
-- Name: market_vendors id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_vendors ALTER COLUMN id SET DEFAULT nextval('public.market_vendors_id_seq'::regclass);


--
-- Name: meeting_tokens id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_tokens ALTER COLUMN id SET DEFAULT nextval('public.meeting_tokens_id_seq'::regclass);


--
-- Name: membership_funds id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.membership_funds ALTER COLUMN id SET DEFAULT nextval('public.membership_funds_id_seq'::regclass);


--
-- Name: membership_waitlist id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.membership_waitlist ALTER COLUMN id SET DEFAULT nextval('public.membership_waitlist_id_seq'::regclass);


--
-- Name: memberships id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memberships ALTER COLUMN id SET DEFAULT nextval('public.memberships_id_seq'::regclass);


--
-- Name: memory_requests id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memory_requests ALTER COLUMN id SET DEFAULT nextval('public.memory_requests_id_seq'::regclass);


--
-- Name: messages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages ALTER COLUMN id SET DEFAULT nextval('public.messages_id_seq'::regclass);


--
-- Name: nfc_connections id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.nfc_connections ALTER COLUMN id SET DEFAULT nextval('public.nfc_connections_id_seq'::regclass);


--
-- Name: node_applications id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.node_applications ALTER COLUMN id SET DEFAULT nextval('public.node_applications_id_seq'::regclass);


--
-- Name: notifications id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications ALTER COLUMN id SET DEFAULT nextval('public.notifications_id_seq'::regclass);


--
-- Name: one_time_pre_keys id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.one_time_pre_keys ALTER COLUMN id SET DEFAULT nextval('public.one_time_pre_keys_id_seq'::regclass);


--
-- Name: order_splits id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.order_splits ALTER COLUMN id SET DEFAULT nextval('public.order_splits_id_seq'::regclass);


--
-- Name: orders id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.orders ALTER COLUMN id SET DEFAULT nextval('public.orders_id_seq'::regclass);


--
-- Name: pending_connections id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pending_connections ALTER COLUMN id SET DEFAULT nextval('public.pending_connections_id_seq'::regclass);


--
-- Name: personal_toilets id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.personal_toilets ALTER COLUMN id SET DEFAULT nextval('public.personal_toilets_id_seq'::regclass);


--
-- Name: personalized_menus id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.personalized_menus ALTER COLUMN id SET DEFAULT nextval('public.personalized_menus_id_seq'::regclass);


--
-- Name: platform_messages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.platform_messages ALTER COLUMN id SET DEFAULT nextval('public.platform_messages_id_seq'::regclass);


--
-- Name: popup_checkins id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_checkins ALTER COLUMN id SET DEFAULT nextval('public.popup_checkins_id_seq'::regclass);


--
-- Name: popup_food_orders id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_food_orders ALTER COLUMN id SET DEFAULT nextval('public.popup_food_orders_id_seq'::regclass);


--
-- Name: popup_merch_items id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_merch_items ALTER COLUMN id SET DEFAULT nextval('public.popup_merch_items_id_seq'::regclass);


--
-- Name: popup_merch_orders id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_merch_orders ALTER COLUMN id SET DEFAULT nextval('public.popup_merch_orders_id_seq'::regclass);


--
-- Name: popup_nominations id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_nominations ALTER COLUMN id SET DEFAULT nextval('public.popup_nominations_id_seq'::regclass);


--
-- Name: popup_requests id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_requests ALTER COLUMN id SET DEFAULT nextval('public.popup_requests_id_seq'::regclass);


--
-- Name: popup_rsvps id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_rsvps ALTER COLUMN id SET DEFAULT nextval('public.popup_rsvps_id_seq'::regclass);


--
-- Name: portal_access id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portal_access ALTER COLUMN id SET DEFAULT nextval('public.portal_access_id_seq'::regclass);


--
-- Name: portal_consents id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portal_consents ALTER COLUMN id SET DEFAULT nextval('public.portal_consents_id_seq'::regclass);


--
-- Name: portal_content id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portal_content ALTER COLUMN id SET DEFAULT nextval('public.portal_content_id_seq'::regclass);


--
-- Name: portrait_license_requests id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_license_requests ALTER COLUMN id SET DEFAULT nextval('public.portrait_license_requests_id_seq'::regclass);


--
-- Name: portrait_licenses id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_licenses ALTER COLUMN id SET DEFAULT nextval('public.portrait_licenses_id_seq'::regclass);


--
-- Name: portrait_token_listings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_token_listings ALTER COLUMN id SET DEFAULT nextval('public.portrait_token_listings_id_seq'::regclass);


--
-- Name: portrait_tokens id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_tokens ALTER COLUMN id SET DEFAULT nextval('public.portrait_tokens_id_seq'::regclass);


--
-- Name: portraits id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portraits ALTER COLUMN id SET DEFAULT nextval('public.portraits_id_seq'::regclass);


--
-- Name: preorders id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.preorders ALTER COLUMN id SET DEFAULT nextval('public.preorders_id_seq'::regclass);


--
-- Name: product_bundles id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.product_bundles ALTER COLUMN id SET DEFAULT nextval('public.product_bundles_id_seq'::regclass);


--
-- Name: promotion_deliveries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.promotion_deliveries ALTER COLUMN id SET DEFAULT nextval('public.promotion_deliveries_id_seq'::regclass);


--
-- Name: provenance_tokens id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provenance_tokens ALTER COLUMN id SET DEFAULT nextval('public.provenance_tokens_id_seq'::regclass);


--
-- Name: referral_codes id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.referral_codes ALTER COLUMN id SET DEFAULT nextval('public.referral_codes_id_seq'::regclass);


--
-- Name: referrals id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.referrals ALTER COLUMN id SET DEFAULT nextval('public.referrals_id_seq'::regclass);


--
-- Name: reservation_bookings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reservation_bookings ALTER COLUMN id SET DEFAULT nextval('public.reservation_bookings_id_seq'::regclass);


--
-- Name: reservation_offers id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reservation_offers ALTER COLUMN id SET DEFAULT nextval('public.reservation_offers_id_seq'::regclass);


--
-- Name: season_patronages id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.season_patronages ALTER COLUMN id SET DEFAULT nextval('public.season_patronages_id_seq'::regclass);


--
-- Name: staff_sessions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.staff_sessions ALTER COLUMN id SET DEFAULT nextval('public.staff_sessions_id_seq'::regclass);


--
-- Name: standing_order_tiers id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_order_tiers ALTER COLUMN id SET DEFAULT nextval('public.standing_order_tiers_id_seq'::regclass);


--
-- Name: standing_order_transfers id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_order_transfers ALTER COLUMN id SET DEFAULT nextval('public.standing_order_transfers_id_seq'::regclass);


--
-- Name: standing_order_waitlist id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_order_waitlist ALTER COLUMN id SET DEFAULT nextval('public.standing_order_waitlist_id_seq'::regclass);


--
-- Name: standing_orders id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_orders ALTER COLUMN id SET DEFAULT nextval('public.standing_orders_id_seq'::regclass);


--
-- Name: table_booking_tokens id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_booking_tokens ALTER COLUMN id SET DEFAULT nextval('public.table_booking_tokens_id_seq'::regclass);


--
-- Name: table_bookings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_bookings ALTER COLUMN id SET DEFAULT nextval('public.table_bookings_id_seq'::regclass);


--
-- Name: table_events id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_events ALTER COLUMN id SET DEFAULT nextval('public.table_events_id_seq'::regclass);


--
-- Name: table_instructors id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_instructors ALTER COLUMN id SET DEFAULT nextval('public.table_instructors_id_seq'::regclass);


--
-- Name: table_memberships id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_memberships ALTER COLUMN id SET DEFAULT nextval('public.table_memberships_id_seq'::regclass);


--
-- Name: table_venue_sessions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_venue_sessions ALTER COLUMN id SET DEFAULT nextval('public.table_venue_sessions_id_seq'::regclass);


--
-- Name: table_venues id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_venues ALTER COLUMN id SET DEFAULT nextval('public.table_venues_id_seq'::regclass);


--
-- Name: tasting_feed_reactions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasting_feed_reactions ALTER COLUMN id SET DEFAULT nextval('public.tasting_feed_reactions_id_seq'::regclass);


--
-- Name: tasting_journal id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasting_journal ALTER COLUMN id SET DEFAULT nextval('public.tasting_journal_id_seq'::regclass);


--
-- Name: time_slots id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.time_slots ALTER COLUMN id SET DEFAULT nextval('public.time_slots_id_seq'::regclass);


--
-- Name: toilet_visits id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.toilet_visits ALTER COLUMN id SET DEFAULT nextval('public.toilet_visits_id_seq'::regclass);


--
-- Name: user_business_visits id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_business_visits ALTER COLUMN id SET DEFAULT nextval('public.user_business_visits_id_seq'::regclass);


--
-- Name: user_challenge_progress id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_challenge_progress ALTER COLUMN id SET DEFAULT nextval('public.user_challenge_progress_id_seq'::regclass);


--
-- Name: user_earnings id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_earnings ALTER COLUMN id SET DEFAULT nextval('public.user_earnings_id_seq'::regclass);


--
-- Name: user_follows id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_follows ALTER COLUMN id SET DEFAULT nextval('public.user_follows_id_seq'::regclass);


--
-- Name: user_keys id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_keys ALTER COLUMN id SET DEFAULT nextval('public.user_keys_id_seq'::regclass);


--
-- Name: user_map_entries id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_map_entries ALTER COLUMN id SET DEFAULT nextval('public.user_map_entries_id_seq'::regclass);


--
-- Name: user_maps id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_maps ALTER COLUMN id SET DEFAULT nextval('public.user_maps_id_seq'::regclass);


--
-- Name: user_saves id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_saves ALTER COLUMN id SET DEFAULT nextval('public.user_saves_id_seq'::regclass);


--
-- Name: users id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users ALTER COLUMN id SET DEFAULT nextval('public.users_id_seq'::regclass);


--
-- Name: varieties id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.varieties ALTER COLUMN id SET DEFAULT nextval('public.varieties_id_seq'::regclass);


--
-- Name: variety_drops id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_drops ALTER COLUMN id SET DEFAULT nextval('public.variety_drops_id_seq'::regclass);


--
-- Name: variety_profiles id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_profiles ALTER COLUMN id SET DEFAULT nextval('public.variety_profiles_id_seq'::regclass);


--
-- Name: variety_reviews id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_reviews ALTER COLUMN id SET DEFAULT nextval('public.variety_reviews_id_seq'::regclass);


--
-- Name: variety_seasons id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_seasons ALTER COLUMN id SET DEFAULT nextval('public.variety_seasons_id_seq'::regclass);


--
-- Name: verification_payments id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.verification_payments ALTER COLUMN id SET DEFAULT nextval('public.verification_payments_id_seq'::regclass);


--
-- Name: walk_in_tokens id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.walk_in_tokens ALTER COLUMN id SET DEFAULT nextval('public.walk_in_tokens_id_seq'::regclass);


--
-- Name: webhook_subscriptions id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhook_subscriptions ALTER COLUMN id SET DEFAULT nextval('public.webhook_subscriptions_id_seq'::regclass);


--

--
-- Name: ad_campaigns ad_campaigns_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ad_campaigns
    ADD CONSTRAINT ad_campaigns_pkey PRIMARY KEY (id);


--
-- Name: ad_impressions ad_impressions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ad_impressions
    ADD CONSTRAINT ad_impressions_pkey PRIMARY KEY (id);


--
-- Name: akene_events akene_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.akene_events
    ADD CONSTRAINT akene_events_pkey PRIMARY KEY (id);


--
-- Name: akene_invitations akene_invitations_event_id_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.akene_invitations
    ADD CONSTRAINT akene_invitations_event_id_user_id_key UNIQUE (event_id, user_id);


--
-- Name: akene_invitations akene_invitations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.akene_invitations
    ADD CONSTRAINT akene_invitations_pkey PRIMARY KEY (id);


--
-- Name: akene_purchases akene_purchases_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.akene_purchases
    ADD CONSTRAINT akene_purchases_pkey PRIMARY KEY (id);


--
-- Name: akene_purchases akene_purchases_stripe_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.akene_purchases
    ADD CONSTRAINT akene_purchases_stripe_payment_intent_id_key UNIQUE (stripe_payment_intent_id);


--
-- Name: ar_notes ar_notes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ar_notes
    ADD CONSTRAINT ar_notes_pkey PRIMARY KEY (id);


--
-- Name: art_acquisitions art_acquisitions_artwork_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_acquisitions
    ADD CONSTRAINT art_acquisitions_artwork_id_key UNIQUE (artwork_id);


--
-- Name: art_acquisitions art_acquisitions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_acquisitions
    ADD CONSTRAINT art_acquisitions_pkey PRIMARY KEY (id);


--
-- Name: art_auctions art_auctions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_auctions
    ADD CONSTRAINT art_auctions_pkey PRIMARY KEY (id);


--
-- Name: art_bids art_bids_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_bids
    ADD CONSTRAINT art_bids_pkey PRIMARY KEY (id);


--
-- Name: art_management_fees art_management_fees_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_management_fees
    ADD CONSTRAINT art_management_fees_pkey PRIMARY KEY (id);


--
-- Name: art_pitches art_pitches_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_pitches
    ADD CONSTRAINT art_pitches_pkey PRIMARY KEY (id);


--
-- Name: artworks artworks_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.artworks
    ADD CONSTRAINT artworks_pkey PRIMARY KEY (id);


--
-- Name: attest_challenges attest_challenges_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.attest_challenges
    ADD CONSTRAINT attest_challenges_pkey PRIMARY KEY (challenge);


--
-- Name: batch_preferences batch_preferences_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_preferences
    ADD CONSTRAINT batch_preferences_pkey PRIMARY KEY (id);


--
-- Name: batch_preferences batch_preferences_user_id_variety_id_location_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_preferences
    ADD CONSTRAINT batch_preferences_user_id_variety_id_location_id_key UNIQUE (user_id, variety_id, location_id);


--
-- Name: batches batches_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batches
    ADD CONSTRAINT batches_pkey PRIMARY KEY (id);


--
-- Name: beacons beacons_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.beacons
    ADD CONSTRAINT beacons_pkey PRIMARY KEY (id);


--
-- Name: bundle_orders bundle_orders_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_orders
    ADD CONSTRAINT bundle_orders_pkey PRIMARY KEY (id);


--
-- Name: bundle_varieties bundle_varieties_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_varieties
    ADD CONSTRAINT bundle_varieties_pkey PRIMARY KEY (id);


--
-- Name: business_accounts business_accounts_apple_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_accounts
    ADD CONSTRAINT business_accounts_apple_id_key UNIQUE (apple_id);


--
-- Name: business_accounts business_accounts_email_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_accounts
    ADD CONSTRAINT business_accounts_email_key UNIQUE (email);


--
-- Name: business_accounts business_accounts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_accounts
    ADD CONSTRAINT business_accounts_pkey PRIMARY KEY (id);


--
-- Name: business_accounts business_accounts_slug_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_accounts
    ADD CONSTRAINT business_accounts_slug_key UNIQUE (slug);


--
-- Name: business_menu_items business_menu_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_menu_items
    ADD CONSTRAINT business_menu_items_pkey PRIMARY KEY (id);


--
-- Name: business_promotions business_promotions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_promotions
    ADD CONSTRAINT business_promotions_pkey PRIMARY KEY (id);


--
-- Name: business_proposals business_proposals_claim_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_proposals
    ADD CONSTRAINT business_proposals_claim_token_key UNIQUE (claim_token);


--
-- Name: business_proposals business_proposals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_proposals
    ADD CONSTRAINT business_proposals_pkey PRIMARY KEY (id);


--
-- Name: business_visits business_visits_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_visits
    ADD CONSTRAINT business_visits_pkey PRIMARY KEY (id);


--
-- Name: businesses businesses_beacon_uuid_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.businesses
    ADD CONSTRAINT businesses_beacon_uuid_key UNIQUE (beacon_uuid);


--
-- Name: businesses businesses_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.businesses
    ADD CONSTRAINT businesses_pkey PRIMARY KEY (id);


--
-- Name: campaign_commissions campaign_commissions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.campaign_commissions
    ADD CONSTRAINT campaign_commissions_pkey PRIMARY KEY (id);


--
-- Name: campaign_commissions campaign_commissions_stripe_payment_intent_id_unique; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.campaign_commissions
    ADD CONSTRAINT campaign_commissions_stripe_payment_intent_id_unique UNIQUE (stripe_payment_intent_id);


--
-- Name: campaign_signups campaign_signups_campaign_id_user_id_unique; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.campaign_signups
    ADD CONSTRAINT campaign_signups_campaign_id_user_id_unique UNIQUE (campaign_id, user_id);


--
-- Name: campaign_signups campaign_signups_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.campaign_signups
    ADD CONSTRAINT campaign_signups_pkey PRIMARY KEY (id);


--
-- Name: campaigns campaigns_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.campaigns
    ADD CONSTRAINT campaigns_pkey PRIMARY KEY (id);


--
-- Name: co_scans co_scans_initiator_code_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.co_scans
    ADD CONSTRAINT co_scans_initiator_code_key UNIQUE (initiator_code);


--
-- Name: co_scans co_scans_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.co_scans
    ADD CONSTRAINT co_scans_pkey PRIMARY KEY (id);


--
-- Name: collectif_challenges collectif_challenges_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collectif_challenges
    ADD CONSTRAINT collectif_challenges_pkey PRIMARY KEY (id);


--
-- Name: collectif_commitments collectif_commitments_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collectif_commitments
    ADD CONSTRAINT collectif_commitments_payment_intent_id_key UNIQUE (payment_intent_id);


--
-- Name: collectif_commitments collectif_commitments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collectif_commitments
    ADD CONSTRAINT collectif_commitments_pkey PRIMARY KEY (id);


--
-- Name: collectifs collectifs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collectifs
    ADD CONSTRAINT collectifs_pkey PRIMARY KEY (id);


--
-- Name: community_events community_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.community_events
    ADD CONSTRAINT community_events_pkey PRIMARY KEY (id);


--
-- Name: community_fund_contributions community_fund_contributions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.community_fund_contributions
    ADD CONSTRAINT community_fund_contributions_pkey PRIMARY KEY (id);


--
-- Name: community_fund_contributions community_fund_contributions_stripe_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.community_fund_contributions
    ADD CONSTRAINT community_fund_contributions_stripe_payment_intent_id_key UNIQUE (stripe_payment_intent_id);


--
-- Name: community_fund community_fund_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.community_fund
    ADD CONSTRAINT community_fund_pkey PRIMARY KEY (id);


--
-- Name: community_popup_interest community_popup_interest_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.community_popup_interest
    ADD CONSTRAINT community_popup_interest_pkey PRIMARY KEY (id);


--
-- Name: connections connections_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.connections
    ADD CONSTRAINT connections_pkey PRIMARY KEY (id);


--
-- Name: connections connections_user_a_id_user_b_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.connections
    ADD CONSTRAINT connections_user_a_id_user_b_id_key UNIQUE (user_a_id, user_b_id);


--
-- Name: contract_requests contract_requests_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contract_requests
    ADD CONSTRAINT contract_requests_pkey PRIMARY KEY (id);


--
-- Name: conversation_archives conversation_archives_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.conversation_archives
    ADD CONSTRAINT conversation_archives_pkey PRIMARY KEY (id);


--
-- Name: conversation_archives conversation_archives_user_id_other_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.conversation_archives
    ADD CONSTRAINT conversation_archives_user_id_other_user_id_key UNIQUE (user_id, other_user_id);


--
-- Name: corporate_accounts corporate_accounts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.corporate_accounts
    ADD CONSTRAINT corporate_accounts_pkey PRIMARY KEY (id);


--
-- Name: corporate_members corporate_members_corporate_id_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.corporate_members
    ADD CONSTRAINT corporate_members_corporate_id_user_id_key UNIQUE (corporate_id, user_id);


--
-- Name: corporate_members corporate_members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.corporate_members
    ADD CONSTRAINT corporate_members_pkey PRIMARY KEY (id);


--
-- Name: credit_transactions credit_transactions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.credit_transactions
    ADD CONSTRAINT credit_transactions_pkey PRIMARY KEY (id);


--
-- Name: credit_transactions credit_transactions_stripe_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.credit_transactions
    ADD CONSTRAINT credit_transactions_stripe_payment_intent_id_key UNIQUE (stripe_payment_intent_id);


--
-- Name: date_invitations date_invitations_offer_id_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_invitations
    ADD CONSTRAINT date_invitations_offer_id_user_id_key UNIQUE (offer_id, user_id);


--
-- Name: date_invitations date_invitations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_invitations
    ADD CONSTRAINT date_invitations_pkey PRIMARY KEY (id);


--
-- Name: date_matches date_matches_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_matches
    ADD CONSTRAINT date_matches_pkey PRIMARY KEY (id);


--
-- Name: date_offers date_offers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_offers
    ADD CONSTRAINT date_offers_pkey PRIMARY KEY (id);


--
-- Name: device_attestations device_attestations_key_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_attestations
    ADD CONSTRAINT device_attestations_key_id_key UNIQUE (key_id);


--
-- Name: device_attestations device_attestations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_attestations
    ADD CONSTRAINT device_attestations_pkey PRIMARY KEY (id);


--
-- Name: device_pairing_tokens device_pairing_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_pairing_tokens
    ADD CONSTRAINT device_pairing_tokens_pkey PRIMARY KEY (id);


--
-- Name: device_pairing_tokens device_pairing_tokens_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_pairing_tokens
    ADD CONSTRAINT device_pairing_tokens_token_key UNIQUE (token);


--
-- Name: devices devices_device_address_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.devices
    ADD CONSTRAINT devices_device_address_key UNIQUE (device_address);


--
-- Name: devices devices_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.devices
    ADD CONSTRAINT devices_pkey PRIMARY KEY (id);


--
-- Name: dj_offers dj_offers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.dj_offers
    ADD CONSTRAINT dj_offers_pkey PRIMARY KEY (id);


--
-- Name: drop_claims drop_claims_drop_id_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.drop_claims
    ADD CONSTRAINT drop_claims_drop_id_user_id_key UNIQUE (drop_id, user_id);


--
-- Name: drop_claims drop_claims_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.drop_claims
    ADD CONSTRAINT drop_claims_pkey PRIMARY KEY (id);


--
-- Name: drop_waitlist drop_waitlist_drop_id_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.drop_waitlist
    ADD CONSTRAINT drop_waitlist_drop_id_user_id_key UNIQUE (drop_id, user_id);


--
-- Name: drop_waitlist drop_waitlist_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.drop_waitlist
    ADD CONSTRAINT drop_waitlist_pkey PRIMARY KEY (id);


--
-- Name: drops drops_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.drops
    ADD CONSTRAINT drops_pkey PRIMARY KEY (id);


--
-- Name: editorial_pieces editorial_pieces_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.editorial_pieces
    ADD CONSTRAINT editorial_pieces_pkey PRIMARY KEY (id);


--
-- Name: employment_contracts employment_contracts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.employment_contracts
    ADD CONSTRAINT employment_contracts_pkey PRIMARY KEY (id);


--
-- Name: evening_tokens evening_tokens_booking_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.evening_tokens
    ADD CONSTRAINT evening_tokens_booking_id_key UNIQUE (booking_id);


--
-- Name: evening_tokens evening_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.evening_tokens
    ADD CONSTRAINT evening_tokens_pkey PRIMARY KEY (id);


--
-- Name: explicit_portals explicit_portals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.explicit_portals
    ADD CONSTRAINT explicit_portals_pkey PRIMARY KEY (id);


--
-- Name: explicit_portals explicit_portals_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.explicit_portals
    ADD CONSTRAINT explicit_portals_user_id_key UNIQUE (user_id);


--
-- Name: farm_visit_bookings farm_visit_bookings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.farm_visit_bookings
    ADD CONSTRAINT farm_visit_bookings_pkey PRIMARY KEY (id);


--
-- Name: farm_visit_bookings farm_visit_bookings_visit_id_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.farm_visit_bookings
    ADD CONSTRAINT farm_visit_bookings_visit_id_user_id_key UNIQUE (visit_id, user_id);


--
-- Name: farm_visits farm_visits_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.farm_visits
    ADD CONSTRAINT farm_visits_pkey PRIMARY KEY (id);


--
-- Name: fraise_business_sessions fraise_business_sessions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_business_sessions
    ADD CONSTRAINT fraise_business_sessions_pkey PRIMARY KEY (id);


--
-- Name: fraise_business_sessions fraise_business_sessions_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_business_sessions
    ADD CONSTRAINT fraise_business_sessions_token_key UNIQUE (token);


--
-- Name: fraise_businesses fraise_businesses_email_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_businesses
    ADD CONSTRAINT fraise_businesses_email_key UNIQUE (email);


--
-- Name: fraise_businesses fraise_businesses_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_businesses
    ADD CONSTRAINT fraise_businesses_pkey PRIMARY KEY (id);


--
-- Name: fraise_businesses fraise_businesses_slug_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_businesses
    ADD CONSTRAINT fraise_businesses_slug_key UNIQUE (slug);


--
-- Name: fraise_claims fraise_claims_confirm_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_claims
    ADD CONSTRAINT fraise_claims_confirm_token_key UNIQUE (confirm_token);


--
-- Name: fraise_claims fraise_claims_member_id_event_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_claims
    ADD CONSTRAINT fraise_claims_member_id_event_id_key UNIQUE (member_id, event_id);


--
-- Name: fraise_claims fraise_claims_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_claims
    ADD CONSTRAINT fraise_claims_pkey PRIMARY KEY (id);


--
-- Name: fraise_credit_purchases fraise_credit_purchases_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_credit_purchases
    ADD CONSTRAINT fraise_credit_purchases_pkey PRIMARY KEY (id);


--
-- Name: fraise_credit_purchases fraise_credit_purchases_stripe_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_credit_purchases
    ADD CONSTRAINT fraise_credit_purchases_stripe_payment_intent_id_key UNIQUE (stripe_payment_intent_id);


--
-- Name: fraise_events fraise_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_events
    ADD CONSTRAINT fraise_events_pkey PRIMARY KEY (id);


--
-- Name: fraise_interest fraise_interest_business_id_email_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_interest
    ADD CONSTRAINT fraise_interest_business_id_email_key UNIQUE (business_id, email);


--
-- Name: fraise_interest fraise_interest_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_interest
    ADD CONSTRAINT fraise_interest_pkey PRIMARY KEY (id);


--
-- Name: fraise_invitations fraise_invitations_confirm_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_invitations
    ADD CONSTRAINT fraise_invitations_confirm_token_key UNIQUE (confirm_token);


--
-- Name: fraise_invitations fraise_invitations_event_id_member_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_invitations
    ADD CONSTRAINT fraise_invitations_event_id_member_id_key UNIQUE (event_id, member_id);


--
-- Name: fraise_invitations fraise_invitations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_invitations
    ADD CONSTRAINT fraise_invitations_pkey PRIMARY KEY (id);


--
-- Name: fraise_member_resets fraise_member_resets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_member_resets
    ADD CONSTRAINT fraise_member_resets_pkey PRIMARY KEY (id);


--
-- Name: fraise_member_sessions fraise_member_sessions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_member_sessions
    ADD CONSTRAINT fraise_member_sessions_pkey PRIMARY KEY (id);


--
-- Name: fraise_member_sessions fraise_member_sessions_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_member_sessions
    ADD CONSTRAINT fraise_member_sessions_token_key UNIQUE (token);


--
-- Name: fraise_members fraise_members_apple_sub_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_members
    ADD CONSTRAINT fraise_members_apple_sub_key UNIQUE (apple_sub);


--
-- Name: fraise_members fraise_members_email_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_members
    ADD CONSTRAINT fraise_members_email_key UNIQUE (email);


--
-- Name: fraise_members fraise_members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_members
    ADD CONSTRAINT fraise_members_pkey PRIMARY KEY (id);


--
-- Name: fraise_messages fraise_messages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_messages
    ADD CONSTRAINT fraise_messages_pkey PRIMARY KEY (id);


--
-- Name: fund_contributions fund_contributions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fund_contributions
    ADD CONSTRAINT fund_contributions_pkey PRIMARY KEY (id);


--
-- Name: fund_contributions fund_contributions_stripe_payment_intent_id_unique; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fund_contributions
    ADD CONSTRAINT fund_contributions_stripe_payment_intent_id_unique UNIQUE (stripe_payment_intent_id);


--
-- Name: gift_registry gift_registry_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gift_registry
    ADD CONSTRAINT gift_registry_pkey PRIMARY KEY (id);


--
-- Name: gift_registry gift_registry_user_id_variety_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gift_registry
    ADD CONSTRAINT gift_registry_user_id_variety_id_key UNIQUE (user_id, variety_id);


--
-- Name: gifts gifts_claim_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gifts
    ADD CONSTRAINT gifts_claim_token_key UNIQUE (claim_token);


--
-- Name: gifts gifts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gifts
    ADD CONSTRAINT gifts_pkey PRIMARY KEY (id);


--
-- Name: greenhouse_funding greenhouse_funding_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.greenhouse_funding
    ADD CONSTRAINT greenhouse_funding_pkey PRIMARY KEY (id);


--
-- Name: greenhouse_funding greenhouse_funding_stripe_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.greenhouse_funding
    ADD CONSTRAINT greenhouse_funding_stripe_payment_intent_id_key UNIQUE (stripe_payment_intent_id);


--
-- Name: greenhouses greenhouses_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.greenhouses
    ADD CONSTRAINT greenhouses_pkey PRIMARY KEY (id);


--
-- Name: harvest_logs harvest_logs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.harvest_logs
    ADD CONSTRAINT harvest_logs_pkey PRIMARY KEY (id);


--
-- Name: health_profiles health_profiles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.health_profiles
    ADD CONSTRAINT health_profiles_pkey PRIMARY KEY (id);


--
-- Name: health_profiles health_profiles_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.health_profiles
    ADD CONSTRAINT health_profiles_user_id_key UNIQUE (user_id);


--
-- Name: id_attestation_log id_attestation_log_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.id_attestation_log
    ADD CONSTRAINT id_attestation_log_pkey PRIMARY KEY (id);


--
-- Name: itineraries itineraries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itineraries
    ADD CONSTRAINT itineraries_pkey PRIMARY KEY (id);


--
-- Name: itinerary_destinations itinerary_destinations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itinerary_destinations
    ADD CONSTRAINT itinerary_destinations_pkey PRIMARY KEY (id);


--
-- Name: itinerary_proposals itinerary_proposals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itinerary_proposals
    ADD CONSTRAINT itinerary_proposals_pkey PRIMARY KEY (id);


--
-- Name: job_applications job_applications_job_id_applicant_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_applications
    ADD CONSTRAINT job_applications_job_id_applicant_id_key UNIQUE (job_id, applicant_id);


--
-- Name: job_applications job_applications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_applications
    ADD CONSTRAINT job_applications_pkey PRIMARY KEY (id);


--
-- Name: job_interviews job_interviews_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_interviews
    ADD CONSTRAINT job_interviews_pkey PRIMARY KEY (id);


--
-- Name: job_ledger_entries job_ledger_entries_application_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_ledger_entries
    ADD CONSTRAINT job_ledger_entries_application_id_key UNIQUE (application_id);


--
-- Name: job_ledger_entries job_ledger_entries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_ledger_entries
    ADD CONSTRAINT job_ledger_entries_pkey PRIMARY KEY (id);


--
-- Name: job_postings job_postings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_postings
    ADD CONSTRAINT job_postings_pkey PRIMARY KEY (id);


--
-- Name: key_challenges key_challenges_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.key_challenges
    ADD CONSTRAINT key_challenges_pkey PRIMARY KEY (id);


--
-- Name: kommune_assignments kommune_assignments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.kommune_assignments
    ADD CONSTRAINT kommune_assignments_pkey PRIMARY KEY (id);


--
-- Name: kommune_flavour_suggestions kommune_flavour_suggestions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.kommune_flavour_suggestions
    ADD CONSTRAINT kommune_flavour_suggestions_pkey PRIMARY KEY (id);


--
-- Name: kommune_press_applications kommune_press_applications_personal_code_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.kommune_press_applications
    ADD CONSTRAINT kommune_press_applications_personal_code_key UNIQUE (personal_code);


--
-- Name: kommune_press_applications kommune_press_applications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.kommune_press_applications
    ADD CONSTRAINT kommune_press_applications_pkey PRIMARY KEY (id);


--
-- Name: kommune_ratings kommune_ratings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.kommune_ratings
    ADD CONSTRAINT kommune_ratings_pkey PRIMARY KEY (id);


--
-- Name: kommune_reservations kommune_reservations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.kommune_reservations
    ADD CONSTRAINT kommune_reservations_pkey PRIMARY KEY (id);


--
-- Name: legitimacy_events legitimacy_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.legitimacy_events
    ADD CONSTRAINT legitimacy_events_pkey PRIMARY KEY (id);


--
-- Name: location_funding location_funding_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.location_funding
    ADD CONSTRAINT location_funding_pkey PRIMARY KEY (id);


--
-- Name: location_funding location_funding_stripe_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.location_funding
    ADD CONSTRAINT location_funding_stripe_payment_intent_id_key UNIQUE (stripe_payment_intent_id);


--
-- Name: location_staff location_staff_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.location_staff
    ADD CONSTRAINT location_staff_pkey PRIMARY KEY (id);


--
-- Name: location_staff location_staff_user_id_location_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.location_staff
    ADD CONSTRAINT location_staff_user_id_location_id_key UNIQUE (user_id, location_id);


--
-- Name: locations locations_beacon_uuid_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.locations
    ADD CONSTRAINT locations_beacon_uuid_key UNIQUE (beacon_uuid);


--
-- Name: locations locations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.locations
    ADD CONSTRAINT locations_pkey PRIMARY KEY (id);


--
-- Name: market_dates market_dates_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_dates
    ADD CONSTRAINT market_dates_pkey PRIMARY KEY (id);


--
-- Name: market_listings market_listings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_listings
    ADD CONSTRAINT market_listings_pkey PRIMARY KEY (id);


--
-- Name: market_order_items market_order_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_order_items
    ADD CONSTRAINT market_order_items_pkey PRIMARY KEY (id);


--
-- Name: market_orders market_orders_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_orders
    ADD CONSTRAINT market_orders_payment_intent_id_key UNIQUE (payment_intent_id);


--
-- Name: market_orders market_orders_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_orders
    ADD CONSTRAINT market_orders_pkey PRIMARY KEY (id);


--
-- Name: market_orders_v2 market_orders_v2_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_orders_v2
    ADD CONSTRAINT market_orders_v2_pkey PRIMARY KEY (id);


--
-- Name: market_products market_products_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_products
    ADD CONSTRAINT market_products_pkey PRIMARY KEY (id);


--
-- Name: market_stalls market_stalls_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_stalls
    ADD CONSTRAINT market_stalls_pkey PRIMARY KEY (id);


--
-- Name: market_vendors market_vendors_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_vendors
    ADD CONSTRAINT market_vendors_pkey PRIMARY KEY (id);


--
-- Name: market_vendors market_vendors_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_vendors
    ADD CONSTRAINT market_vendors_user_id_key UNIQUE (user_id);


--
-- Name: meeting_tokens meeting_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_tokens
    ADD CONSTRAINT meeting_tokens_pkey PRIMARY KEY (id);


--
-- Name: meeting_tokens meeting_tokens_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_tokens
    ADD CONSTRAINT meeting_tokens_token_key UNIQUE (token);


--
-- Name: membership_funds membership_funds_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.membership_funds
    ADD CONSTRAINT membership_funds_pkey PRIMARY KEY (id);


--
-- Name: membership_funds membership_funds_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.membership_funds
    ADD CONSTRAINT membership_funds_user_id_key UNIQUE (user_id);


--
-- Name: membership_waitlist membership_waitlist_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.membership_waitlist
    ADD CONSTRAINT membership_waitlist_pkey PRIMARY KEY (id);


--
-- Name: memberships memberships_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memberships
    ADD CONSTRAINT memberships_pkey PRIMARY KEY (id);


--
-- Name: memberships memberships_stripe_payment_intent_id_unique; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memberships
    ADD CONSTRAINT memberships_stripe_payment_intent_id_unique UNIQUE (stripe_payment_intent_id);


--
-- Name: memory_requests memory_requests_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memory_requests
    ADD CONSTRAINT memory_requests_pkey PRIMARY KEY (id);


--
-- Name: messages messages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages
    ADD CONSTRAINT messages_pkey PRIMARY KEY (id);


--
-- Name: nfc_connections nfc_connections_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.nfc_connections
    ADD CONSTRAINT nfc_connections_pkey PRIMARY KEY (id);


--
-- Name: nfc_pairing_tokens nfc_pairing_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.nfc_pairing_tokens
    ADD CONSTRAINT nfc_pairing_tokens_pkey PRIMARY KEY (token);


--
-- Name: node_applications node_applications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.node_applications
    ADD CONSTRAINT node_applications_pkey PRIMARY KEY (id);


--
-- Name: notifications notifications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_pkey PRIMARY KEY (id);


--
-- Name: one_time_pre_keys one_time_pre_keys_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.one_time_pre_keys
    ADD CONSTRAINT one_time_pre_keys_pkey PRIMARY KEY (id);


--
-- Name: order_splits order_splits_order_id_split_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.order_splits
    ADD CONSTRAINT order_splits_order_id_split_user_id_key UNIQUE (order_id, split_user_id);


--
-- Name: order_splits order_splits_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.order_splits
    ADD CONSTRAINT order_splits_pkey PRIMARY KEY (id);


--
-- Name: orders orders_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.orders
    ADD CONSTRAINT orders_pkey PRIMARY KEY (id);


--
-- Name: orders orders_stripe_payment_intent_id_unique; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.orders
    ADD CONSTRAINT orders_stripe_payment_intent_id_unique UNIQUE (stripe_payment_intent_id);


--
-- Name: pending_connections pending_connections_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pending_connections
    ADD CONSTRAINT pending_connections_pkey PRIMARY KEY (id);


--
-- Name: pending_connections pending_connections_user_a_id_user_b_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pending_connections
    ADD CONSTRAINT pending_connections_user_a_id_user_b_id_key UNIQUE (user_a_id, user_b_id);


--
-- Name: personal_toilets personal_toilets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.personal_toilets
    ADD CONSTRAINT personal_toilets_pkey PRIMARY KEY (id);


--
-- Name: personalized_menus personalized_menus_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.personalized_menus
    ADD CONSTRAINT personalized_menus_pkey PRIMARY KEY (id);


--
-- Name: platform_messages platform_messages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.platform_messages
    ADD CONSTRAINT platform_messages_pkey PRIMARY KEY (id);


--
-- Name: popup_checkins popup_checkins_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_checkins
    ADD CONSTRAINT popup_checkins_pkey PRIMARY KEY (id);


--
-- Name: popup_food_orders popup_food_orders_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_food_orders
    ADD CONSTRAINT popup_food_orders_pkey PRIMARY KEY (id);


--
-- Name: popup_food_orders popup_food_orders_stripe_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_food_orders
    ADD CONSTRAINT popup_food_orders_stripe_payment_intent_id_key UNIQUE (stripe_payment_intent_id);


--
-- Name: popup_merch_items popup_merch_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_merch_items
    ADD CONSTRAINT popup_merch_items_pkey PRIMARY KEY (id);


--
-- Name: popup_merch_orders popup_merch_orders_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_merch_orders
    ADD CONSTRAINT popup_merch_orders_pkey PRIMARY KEY (id);


--
-- Name: popup_merch_orders popup_merch_orders_stripe_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_merch_orders
    ADD CONSTRAINT popup_merch_orders_stripe_payment_intent_id_key UNIQUE (stripe_payment_intent_id);


--
-- Name: popup_nominations popup_nominations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_nominations
    ADD CONSTRAINT popup_nominations_pkey PRIMARY KEY (id);


--
-- Name: popup_requests popup_requests_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_requests
    ADD CONSTRAINT popup_requests_pkey PRIMARY KEY (id);


--
-- Name: popup_requests popup_requests_stripe_payment_intent_id_unique; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_requests
    ADD CONSTRAINT popup_requests_stripe_payment_intent_id_unique UNIQUE (stripe_payment_intent_id);


--
-- Name: popup_rsvps popup_rsvps_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_rsvps
    ADD CONSTRAINT popup_rsvps_pkey PRIMARY KEY (id);


--
-- Name: popup_rsvps popup_rsvps_stripe_payment_intent_id_unique; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_rsvps
    ADD CONSTRAINT popup_rsvps_stripe_payment_intent_id_unique UNIQUE (stripe_payment_intent_id);


--
-- Name: portal_access portal_access_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portal_access
    ADD CONSTRAINT portal_access_pkey PRIMARY KEY (id);


--
-- Name: portal_access portal_access_stripe_payment_intent_id_unique; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portal_access
    ADD CONSTRAINT portal_access_stripe_payment_intent_id_unique UNIQUE (stripe_payment_intent_id);


--
-- Name: portal_consents portal_consents_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portal_consents
    ADD CONSTRAINT portal_consents_pkey PRIMARY KEY (id);


--
-- Name: portal_consents portal_consents_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portal_consents
    ADD CONSTRAINT portal_consents_user_id_key UNIQUE (user_id);


--
-- Name: portal_content portal_content_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portal_content
    ADD CONSTRAINT portal_content_pkey PRIMARY KEY (id);


--
-- Name: portrait_license_requests portrait_license_requests_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_license_requests
    ADD CONSTRAINT portrait_license_requests_pkey PRIMARY KEY (id);


--
-- Name: portrait_licenses portrait_licenses_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_licenses
    ADD CONSTRAINT portrait_licenses_pkey PRIMARY KEY (id);


--
-- Name: portrait_licenses portrait_licenses_request_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_licenses
    ADD CONSTRAINT portrait_licenses_request_id_key UNIQUE (request_id);


--
-- Name: portrait_token_listings portrait_token_listings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_token_listings
    ADD CONSTRAINT portrait_token_listings_pkey PRIMARY KEY (id);


--
-- Name: portrait_token_listings portrait_token_listings_token_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_token_listings
    ADD CONSTRAINT portrait_token_listings_token_id_key UNIQUE (token_id);


--
-- Name: portrait_tokens portrait_tokens_nfc_uid_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_tokens
    ADD CONSTRAINT portrait_tokens_nfc_uid_key UNIQUE (nfc_uid);


--
-- Name: portrait_tokens portrait_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_tokens
    ADD CONSTRAINT portrait_tokens_pkey PRIMARY KEY (id);


--
-- Name: portraits portraits_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portraits
    ADD CONSTRAINT portraits_pkey PRIMARY KEY (id);


--
-- Name: preorders preorders_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.preorders
    ADD CONSTRAINT preorders_pkey PRIMARY KEY (id);


--
-- Name: product_bundles product_bundles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.product_bundles
    ADD CONSTRAINT product_bundles_pkey PRIMARY KEY (id);


--
-- Name: promotion_deliveries promotion_deliveries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.promotion_deliveries
    ADD CONSTRAINT promotion_deliveries_pkey PRIMARY KEY (id);


--
-- Name: promotion_deliveries promotion_deliveries_promotion_id_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.promotion_deliveries
    ADD CONSTRAINT promotion_deliveries_promotion_id_user_id_key UNIQUE (promotion_id, user_id);


--
-- Name: provenance_tokens provenance_tokens_greenhouse_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provenance_tokens
    ADD CONSTRAINT provenance_tokens_greenhouse_id_key UNIQUE (greenhouse_id);


--
-- Name: provenance_tokens provenance_tokens_nfc_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provenance_tokens
    ADD CONSTRAINT provenance_tokens_nfc_token_key UNIQUE (nfc_token);


--
-- Name: provenance_tokens provenance_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provenance_tokens
    ADD CONSTRAINT provenance_tokens_pkey PRIMARY KEY (id);


--
-- Name: referral_codes referral_codes_code_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.referral_codes
    ADD CONSTRAINT referral_codes_code_key UNIQUE (code);


--
-- Name: referral_codes referral_codes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.referral_codes
    ADD CONSTRAINT referral_codes_pkey PRIMARY KEY (id);


--
-- Name: referrals referrals_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.referrals
    ADD CONSTRAINT referrals_pkey PRIMARY KEY (id);


--
-- Name: referrals referrals_referee_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.referrals
    ADD CONSTRAINT referrals_referee_user_id_key UNIQUE (referee_user_id);


--
-- Name: reservation_bookings reservation_bookings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reservation_bookings
    ADD CONSTRAINT reservation_bookings_pkey PRIMARY KEY (id);


--
-- Name: reservation_offers reservation_offers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reservation_offers
    ADD CONSTRAINT reservation_offers_pkey PRIMARY KEY (id);


--
-- Name: season_patronages season_patronages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.season_patronages
    ADD CONSTRAINT season_patronages_pkey PRIMARY KEY (id);


--
-- Name: season_patronages season_patronages_stripe_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.season_patronages
    ADD CONSTRAINT season_patronages_stripe_payment_intent_id_key UNIQUE (stripe_payment_intent_id);


--
-- Name: staff_sessions staff_sessions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.staff_sessions
    ADD CONSTRAINT staff_sessions_pkey PRIMARY KEY (id);


--
-- Name: staff_sessions staff_sessions_staff_user_id_session_date_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.staff_sessions
    ADD CONSTRAINT staff_sessions_staff_user_id_session_date_key UNIQUE (staff_user_id, session_date);


--
-- Name: standing_order_tiers standing_order_tiers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_order_tiers
    ADD CONSTRAINT standing_order_tiers_pkey PRIMARY KEY (id);


--
-- Name: standing_order_transfers standing_order_transfers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_order_transfers
    ADD CONSTRAINT standing_order_transfers_pkey PRIMARY KEY (id);


--
-- Name: standing_order_waitlist standing_order_waitlist_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_order_waitlist
    ADD CONSTRAINT standing_order_waitlist_pkey PRIMARY KEY (id);


--
-- Name: standing_orders standing_orders_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_orders
    ADD CONSTRAINT standing_orders_pkey PRIMARY KEY (id);


--
-- Name: table_booking_tokens table_booking_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_booking_tokens
    ADD CONSTRAINT table_booking_tokens_pkey PRIMARY KEY (id);


--
-- Name: table_booking_tokens table_booking_tokens_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_booking_tokens
    ADD CONSTRAINT table_booking_tokens_token_key UNIQUE (token);


--
-- Name: table_bookings table_bookings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_bookings
    ADD CONSTRAINT table_bookings_pkey PRIMARY KEY (id);


--
-- Name: table_bookings table_bookings_stripe_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_bookings
    ADD CONSTRAINT table_bookings_stripe_payment_intent_id_key UNIQUE (stripe_payment_intent_id);


--
-- Name: table_events table_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_events
    ADD CONSTRAINT table_events_pkey PRIMARY KEY (id);


--
-- Name: table_instructors table_instructors_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_instructors
    ADD CONSTRAINT table_instructors_pkey PRIMARY KEY (id);


--
-- Name: table_memberships table_memberships_confirm_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_memberships
    ADD CONSTRAINT table_memberships_confirm_token_key UNIQUE (confirm_token);


--
-- Name: table_memberships table_memberships_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_memberships
    ADD CONSTRAINT table_memberships_pkey PRIMARY KEY (id);


--
-- Name: table_memberships table_memberships_stripe_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_memberships
    ADD CONSTRAINT table_memberships_stripe_payment_intent_id_key UNIQUE (stripe_payment_intent_id);


--
-- Name: table_venue_sessions table_venue_sessions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_venue_sessions
    ADD CONSTRAINT table_venue_sessions_pkey PRIMARY KEY (id);


--
-- Name: table_venue_sessions table_venue_sessions_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_venue_sessions
    ADD CONSTRAINT table_venue_sessions_token_key UNIQUE (token);


--
-- Name: table_venues table_venues_email_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_venues
    ADD CONSTRAINT table_venues_email_key UNIQUE (email);


--
-- Name: table_venues table_venues_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_venues
    ADD CONSTRAINT table_venues_pkey PRIMARY KEY (id);


--
-- Name: table_venues table_venues_slug_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_venues
    ADD CONSTRAINT table_venues_slug_key UNIQUE (slug);


--
-- Name: tasting_feed_reactions tasting_feed_reactions_entry_id_user_id_emoji_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasting_feed_reactions
    ADD CONSTRAINT tasting_feed_reactions_entry_id_user_id_emoji_key UNIQUE (entry_id, user_id, emoji);


--
-- Name: tasting_feed_reactions tasting_feed_reactions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasting_feed_reactions
    ADD CONSTRAINT tasting_feed_reactions_pkey PRIMARY KEY (id);


--
-- Name: tasting_journal tasting_journal_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasting_journal
    ADD CONSTRAINT tasting_journal_pkey PRIMARY KEY (id);


--
-- Name: tasting_journal tasting_journal_user_id_variety_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasting_journal
    ADD CONSTRAINT tasting_journal_user_id_variety_id_key UNIQUE (user_id, variety_id);


--
-- Name: time_slots time_slots_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.time_slots
    ADD CONSTRAINT time_slots_pkey PRIMARY KEY (id);


--
-- Name: toilet_visits toilet_visits_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.toilet_visits
    ADD CONSTRAINT toilet_visits_pkey PRIMARY KEY (id);


--
-- Name: typing_indicators typing_indicators_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.typing_indicators
    ADD CONSTRAINT typing_indicators_pkey PRIMARY KEY (user_id, contact_id);


--
-- Name: user_business_visits user_business_visits_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_business_visits
    ADD CONSTRAINT user_business_visits_pkey PRIMARY KEY (id);


--
-- Name: user_challenge_progress user_challenge_progress_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_challenge_progress
    ADD CONSTRAINT user_challenge_progress_pkey PRIMARY KEY (id);


--
-- Name: user_challenge_progress user_challenge_progress_user_id_challenge_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_challenge_progress
    ADD CONSTRAINT user_challenge_progress_user_id_challenge_id_key UNIQUE (user_id, challenge_id);


--
-- Name: user_earnings user_earnings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_earnings
    ADD CONSTRAINT user_earnings_pkey PRIMARY KEY (id);


--
-- Name: user_follows user_follows_follower_id_followee_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_follows
    ADD CONSTRAINT user_follows_follower_id_followee_id_key UNIQUE (follower_id, followee_id);


--
-- Name: user_follows user_follows_follower_id_followee_id_unique; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_follows
    ADD CONSTRAINT user_follows_follower_id_followee_id_unique UNIQUE (follower_id, followee_id);


--
-- Name: user_follows user_follows_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_follows
    ADD CONSTRAINT user_follows_pkey PRIMARY KEY (id);


--
-- Name: user_keys user_keys_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_keys
    ADD CONSTRAINT user_keys_pkey PRIMARY KEY (id);


--
-- Name: user_keys user_keys_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_keys
    ADD CONSTRAINT user_keys_user_id_key UNIQUE (user_id);


--
-- Name: user_map_entries user_map_entries_map_id_business_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_map_entries
    ADD CONSTRAINT user_map_entries_map_id_business_id_key UNIQUE (map_id, business_id);


--
-- Name: user_map_entries user_map_entries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_map_entries
    ADD CONSTRAINT user_map_entries_pkey PRIMARY KEY (id);


--
-- Name: user_maps user_maps_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_maps
    ADD CONSTRAINT user_maps_pkey PRIMARY KEY (id);


--
-- Name: user_saves user_saves_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_saves
    ADD CONSTRAINT user_saves_pkey PRIMARY KEY (id);


--
-- Name: user_saves user_saves_saver_id_saved_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_saves
    ADD CONSTRAINT user_saves_saver_id_saved_user_id_key UNIQUE (saver_id, saved_user_id);


--
-- Name: users users_email_unique; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_email_unique UNIQUE (email);


--
-- Name: users users_eth_address_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_eth_address_key UNIQUE (eth_address);


--
-- Name: users users_fraise_chat_email_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_fraise_chat_email_key UNIQUE (fraise_chat_email);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: users users_user_code_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_user_code_key UNIQUE (user_code);


--
-- Name: varieties varieties_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.varieties
    ADD CONSTRAINT varieties_pkey PRIMARY KEY (id);


--
-- Name: variety_drops variety_drops_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_drops
    ADD CONSTRAINT variety_drops_pkey PRIMARY KEY (id);


--
-- Name: variety_profiles variety_profiles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_profiles
    ADD CONSTRAINT variety_profiles_pkey PRIMARY KEY (id);


--
-- Name: variety_profiles variety_profiles_variety_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_profiles
    ADD CONSTRAINT variety_profiles_variety_id_key UNIQUE (variety_id);


--
-- Name: variety_reviews variety_reviews_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_reviews
    ADD CONSTRAINT variety_reviews_pkey PRIMARY KEY (id);


--
-- Name: variety_reviews variety_reviews_user_id_variety_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_reviews
    ADD CONSTRAINT variety_reviews_user_id_variety_id_key UNIQUE (user_id, variety_id);


--
-- Name: variety_seasons variety_seasons_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_seasons
    ADD CONSTRAINT variety_seasons_pkey PRIMARY KEY (id);


--
-- Name: variety_seasons variety_seasons_variety_id_year_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_seasons
    ADD CONSTRAINT variety_seasons_variety_id_year_key UNIQUE (variety_id, year);


--
-- Name: verification_payments verification_payments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.verification_payments
    ADD CONSTRAINT verification_payments_pkey PRIMARY KEY (id);


--
-- Name: verification_payments verification_payments_stripe_payment_intent_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.verification_payments
    ADD CONSTRAINT verification_payments_stripe_payment_intent_id_key UNIQUE (stripe_payment_intent_id);


--
-- Name: walk_in_tokens walk_in_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.walk_in_tokens
    ADD CONSTRAINT walk_in_tokens_pkey PRIMARY KEY (id);


--
-- Name: walk_in_tokens walk_in_tokens_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.walk_in_tokens
    ADD CONSTRAINT walk_in_tokens_token_key UNIQUE (token);


--
-- Name: webhook_subscriptions webhook_subscriptions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhook_subscriptions
    ADD CONSTRAINT webhook_subscriptions_pkey PRIMARY KEY (id);


--
-- Name: fraise_claims_event_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX fraise_claims_event_idx ON public.fraise_claims USING btree (event_id, status);


--
-- Name: fraise_claims_member_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX fraise_claims_member_idx ON public.fraise_claims USING btree (member_id, status);


--
-- Name: fraise_interest_business_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX fraise_interest_business_idx ON public.fraise_interest USING btree (business_id, created_at DESC);


--
-- Name: fraise_invitations_member_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX fraise_invitations_member_idx ON public.fraise_invitations USING btree (member_id, status);


--
-- Name: gifts_recipient_email_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX gifts_recipient_email_idx ON public.gifts USING btree (recipient_email);


--
-- Name: gifts_sender_user_id_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX gifts_sender_user_id_idx ON public.gifts USING btree (sender_user_id);


--
-- Name: gifts_sticker_business_id_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX gifts_sticker_business_id_idx ON public.gifts USING btree (sticker_business_id);


--
-- Name: idx_attest_challenges_expires; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_attest_challenges_expires ON public.attest_challenges USING btree (expires_at);


--
-- Name: idx_ubv_user_business; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ubv_user_business ON public.user_business_visits USING btree (user_id, business_id);


--
-- Name: key_challenges_user_expires_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX key_challenges_user_expires_idx ON public.key_challenges USING btree (user_id, expires_at);


--
-- Name: messages_order_id_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX messages_order_id_idx ON public.messages USING btree (order_id) WHERE (order_id IS NOT NULL);


--
-- Name: messages_recipient_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX messages_recipient_idx ON public.messages USING btree (recipient_id);


--
-- Name: messages_sender_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX messages_sender_idx ON public.messages USING btree (sender_id);


--
-- Name: nfc_connections_pair_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX nfc_connections_pair_unique ON public.nfc_connections USING btree (LEAST(user_a, user_b), GREATEST(user_a, user_b));


--
-- Name: nfc_pairing_tokens_expires_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX nfc_pairing_tokens_expires_idx ON public.nfc_pairing_tokens USING btree (expires_at);


--
-- Name: orders_customer_email_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX orders_customer_email_idx ON public.orders USING btree (customer_email);


--
-- Name: otpk_user_key_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX otpk_user_key_idx ON public.one_time_pre_keys USING btree (user_id, key_id);


--
-- Name: table_memberships_slug_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX table_memberships_slug_idx ON public.table_memberships USING btree (slug, status, created_at);


--
-- Name: user_maps_user_id_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX user_maps_user_id_unique ON public.user_maps USING btree (user_id);


--
-- Name: users_business_id_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX users_business_id_idx ON public.users USING btree (business_id) WHERE (is_shop = true);


--
-- Name: users_user_code_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX users_user_code_unique ON public.users USING btree (user_code) WHERE (user_code IS NOT NULL);


--
-- Name: ad_campaigns ad_campaigns_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ad_campaigns
    ADD CONSTRAINT ad_campaigns_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: ad_impressions ad_impressions_campaign_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ad_impressions
    ADD CONSTRAINT ad_impressions_campaign_id_fkey FOREIGN KEY (campaign_id) REFERENCES public.ad_campaigns(id);


--
-- Name: ad_impressions ad_impressions_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ad_impressions
    ADD CONSTRAINT ad_impressions_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: akene_events akene_events_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.akene_events
    ADD CONSTRAINT akene_events_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: akene_events akene_events_created_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.akene_events
    ADD CONSTRAINT akene_events_created_by_user_id_fkey FOREIGN KEY (created_by_user_id) REFERENCES public.users(id);


--
-- Name: akene_invitations akene_invitations_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.akene_invitations
    ADD CONSTRAINT akene_invitations_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.akene_events(id);


--
-- Name: akene_invitations akene_invitations_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.akene_invitations
    ADD CONSTRAINT akene_invitations_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: akene_purchases akene_purchases_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.akene_purchases
    ADD CONSTRAINT akene_purchases_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: ar_notes ar_notes_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ar_notes
    ADD CONSTRAINT ar_notes_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: art_acquisitions art_acquisitions_artwork_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_acquisitions
    ADD CONSTRAINT art_acquisitions_artwork_id_fkey FOREIGN KEY (artwork_id) REFERENCES public.artworks(id);


--
-- Name: art_auctions art_auctions_artwork_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_auctions
    ADD CONSTRAINT art_auctions_artwork_id_fkey FOREIGN KEY (artwork_id) REFERENCES public.artworks(id);


--
-- Name: art_bids art_bids_auction_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_bids
    ADD CONSTRAINT art_bids_auction_id_fkey FOREIGN KEY (auction_id) REFERENCES public.art_auctions(id);


--
-- Name: art_bids art_bids_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_bids
    ADD CONSTRAINT art_bids_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: art_management_fees art_management_fees_acquisition_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_management_fees
    ADD CONSTRAINT art_management_fees_acquisition_id_fkey FOREIGN KEY (acquisition_id) REFERENCES public.art_acquisitions(id);


--
-- Name: art_management_fees art_management_fees_collector_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_management_fees
    ADD CONSTRAINT art_management_fees_collector_user_id_fkey FOREIGN KEY (collector_user_id) REFERENCES public.users(id);


--
-- Name: art_pitches art_pitches_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.art_pitches
    ADD CONSTRAINT art_pitches_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: artworks artworks_pitch_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.artworks
    ADD CONSTRAINT artworks_pitch_id_fkey FOREIGN KEY (pitch_id) REFERENCES public.art_pitches(id);


--
-- Name: artworks artworks_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.artworks
    ADD CONSTRAINT artworks_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: batch_preferences batch_preferences_location_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_preferences
    ADD CONSTRAINT batch_preferences_location_id_fkey FOREIGN KEY (location_id) REFERENCES public.locations(id);


--
-- Name: batch_preferences batch_preferences_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_preferences
    ADD CONSTRAINT batch_preferences_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: batch_preferences batch_preferences_variety_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batch_preferences
    ADD CONSTRAINT batch_preferences_variety_id_fkey FOREIGN KEY (variety_id) REFERENCES public.varieties(id);


--
-- Name: batches batches_location_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batches
    ADD CONSTRAINT batches_location_id_fkey FOREIGN KEY (location_id) REFERENCES public.locations(id);


--
-- Name: batches batches_variety_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.batches
    ADD CONSTRAINT batches_variety_id_fkey FOREIGN KEY (variety_id) REFERENCES public.varieties(id);


--
-- Name: beacons beacons_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.beacons
    ADD CONSTRAINT beacons_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: bundle_orders bundle_orders_bundle_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_orders
    ADD CONSTRAINT bundle_orders_bundle_id_fkey FOREIGN KEY (bundle_id) REFERENCES public.product_bundles(id);


--
-- Name: bundle_orders bundle_orders_location_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_orders
    ADD CONSTRAINT bundle_orders_location_id_fkey FOREIGN KEY (location_id) REFERENCES public.locations(id);


--
-- Name: bundle_orders bundle_orders_time_slot_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_orders
    ADD CONSTRAINT bundle_orders_time_slot_id_fkey FOREIGN KEY (time_slot_id) REFERENCES public.time_slots(id);


--
-- Name: bundle_orders bundle_orders_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_orders
    ADD CONSTRAINT bundle_orders_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: bundle_varieties bundle_varieties_bundle_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_varieties
    ADD CONSTRAINT bundle_varieties_bundle_id_fkey FOREIGN KEY (bundle_id) REFERENCES public.product_bundles(id);


--
-- Name: bundle_varieties bundle_varieties_variety_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.bundle_varieties
    ADD CONSTRAINT bundle_varieties_variety_id_fkey FOREIGN KEY (variety_id) REFERENCES public.varieties(id);


--
-- Name: business_menu_items business_menu_items_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_menu_items
    ADD CONSTRAINT business_menu_items_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: business_promotions business_promotions_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_promotions
    ADD CONSTRAINT business_promotions_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: business_promotions business_promotions_created_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_promotions
    ADD CONSTRAINT business_promotions_created_by_user_id_fkey FOREIGN KEY (created_by_user_id) REFERENCES public.users(id);


--
-- Name: business_proposals business_proposals_proposed_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_proposals
    ADD CONSTRAINT business_proposals_proposed_by_user_id_fkey FOREIGN KEY (proposed_by_user_id) REFERENCES public.users(id);


--
-- Name: business_visits business_visits_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_visits
    ADD CONSTRAINT business_visits_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: business_visits business_visits_contracted_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_visits
    ADD CONSTRAINT business_visits_contracted_user_id_fkey FOREIGN KEY (contracted_user_id) REFERENCES public.users(id);


--
-- Name: business_visits business_visits_visitor_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.business_visits
    ADD CONSTRAINT business_visits_visitor_user_id_fkey FOREIGN KEY (visitor_user_id) REFERENCES public.users(id);


--
-- Name: businesses businesses_founding_patron_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.businesses
    ADD CONSTRAINT businesses_founding_patron_id_fkey FOREIGN KEY (founding_patron_id) REFERENCES public.users(id);


--
-- Name: collectif_commitments collectif_commitments_collectif_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collectif_commitments
    ADD CONSTRAINT collectif_commitments_collectif_id_fkey FOREIGN KEY (collectif_id) REFERENCES public.collectifs(id);


--
-- Name: collectif_commitments collectif_commitments_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collectif_commitments
    ADD CONSTRAINT collectif_commitments_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: collectifs collectifs_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collectifs
    ADD CONSTRAINT collectifs_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: collectifs collectifs_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.collectifs
    ADD CONSTRAINT collectifs_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id);


--
-- Name: community_fund_contributions community_fund_contributions_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.community_fund_contributions
    ADD CONSTRAINT community_fund_contributions_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: community_popup_interest community_popup_interest_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.community_popup_interest
    ADD CONSTRAINT community_popup_interest_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: community_popup_interest community_popup_interest_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.community_popup_interest
    ADD CONSTRAINT community_popup_interest_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: connections connections_user_a_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.connections
    ADD CONSTRAINT connections_user_a_id_fkey FOREIGN KEY (user_a_id) REFERENCES public.users(id);


--
-- Name: connections connections_user_b_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.connections
    ADD CONSTRAINT connections_user_b_id_fkey FOREIGN KEY (user_b_id) REFERENCES public.users(id);


--
-- Name: contract_requests contract_requests_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.contract_requests
    ADD CONSTRAINT contract_requests_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: conversation_archives conversation_archives_other_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.conversation_archives
    ADD CONSTRAINT conversation_archives_other_user_id_fkey FOREIGN KEY (other_user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: conversation_archives conversation_archives_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.conversation_archives
    ADD CONSTRAINT conversation_archives_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: corporate_accounts corporate_accounts_admin_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.corporate_accounts
    ADD CONSTRAINT corporate_accounts_admin_user_id_fkey FOREIGN KEY (admin_user_id) REFERENCES public.users(id);


--
-- Name: corporate_members corporate_members_corporate_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.corporate_members
    ADD CONSTRAINT corporate_members_corporate_id_fkey FOREIGN KEY (corporate_id) REFERENCES public.corporate_accounts(id);


--
-- Name: corporate_members corporate_members_invited_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.corporate_members
    ADD CONSTRAINT corporate_members_invited_by_user_id_fkey FOREIGN KEY (invited_by_user_id) REFERENCES public.users(id);


--
-- Name: corporate_members corporate_members_standing_order_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.corporate_members
    ADD CONSTRAINT corporate_members_standing_order_id_fkey FOREIGN KEY (standing_order_id) REFERENCES public.standing_orders(id);


--
-- Name: corporate_members corporate_members_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.corporate_members
    ADD CONSTRAINT corporate_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: credit_transactions credit_transactions_from_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.credit_transactions
    ADD CONSTRAINT credit_transactions_from_user_id_fkey FOREIGN KEY (from_user_id) REFERENCES public.users(id);


--
-- Name: credit_transactions credit_transactions_to_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.credit_transactions
    ADD CONSTRAINT credit_transactions_to_user_id_fkey FOREIGN KEY (to_user_id) REFERENCES public.users(id);


--
-- Name: date_invitations date_invitations_offer_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_invitations
    ADD CONSTRAINT date_invitations_offer_id_fkey FOREIGN KEY (offer_id) REFERENCES public.date_offers(id);


--
-- Name: date_invitations date_invitations_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_invitations
    ADD CONSTRAINT date_invitations_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: date_matches date_matches_offer_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_matches
    ADD CONSTRAINT date_matches_offer_id_fkey FOREIGN KEY (offer_id) REFERENCES public.date_offers(id);


--
-- Name: date_matches date_matches_user_a_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_matches
    ADD CONSTRAINT date_matches_user_a_id_fkey FOREIGN KEY (user_a_id) REFERENCES public.users(id);


--
-- Name: date_matches date_matches_user_b_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_matches
    ADD CONSTRAINT date_matches_user_b_id_fkey FOREIGN KEY (user_b_id) REFERENCES public.users(id);


--
-- Name: date_offers date_offers_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_offers
    ADD CONSTRAINT date_offers_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: date_offers date_offers_created_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.date_offers
    ADD CONSTRAINT date_offers_created_by_user_id_fkey FOREIGN KEY (created_by_user_id) REFERENCES public.users(id);


--
-- Name: device_attestations device_attestations_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_attestations
    ADD CONSTRAINT device_attestations_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: device_pairing_tokens device_pairing_tokens_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.device_pairing_tokens
    ADD CONSTRAINT device_pairing_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: devices devices_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.devices
    ADD CONSTRAINT devices_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: drop_claims drop_claims_drop_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.drop_claims
    ADD CONSTRAINT drop_claims_drop_id_fkey FOREIGN KEY (drop_id) REFERENCES public.variety_drops(id);


--
-- Name: drop_claims drop_claims_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.drop_claims
    ADD CONSTRAINT drop_claims_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: drop_waitlist drop_waitlist_drop_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.drop_waitlist
    ADD CONSTRAINT drop_waitlist_drop_id_fkey FOREIGN KEY (drop_id) REFERENCES public.variety_drops(id);


--
-- Name: drop_waitlist drop_waitlist_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.drop_waitlist
    ADD CONSTRAINT drop_waitlist_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: editorial_pieces editorial_pieces_author_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.editorial_pieces
    ADD CONSTRAINT editorial_pieces_author_user_id_fkey FOREIGN KEY (author_user_id) REFERENCES public.users(id);


--
-- Name: employment_contracts employment_contracts_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.employment_contracts
    ADD CONSTRAINT employment_contracts_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: employment_contracts employment_contracts_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.employment_contracts
    ADD CONSTRAINT employment_contracts_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: evening_tokens evening_tokens_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.evening_tokens
    ADD CONSTRAINT evening_tokens_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: evening_tokens evening_tokens_offer_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.evening_tokens
    ADD CONSTRAINT evening_tokens_offer_id_fkey FOREIGN KEY (offer_id) REFERENCES public.reservation_offers(id);


--
-- Name: evening_tokens evening_tokens_user_a_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.evening_tokens
    ADD CONSTRAINT evening_tokens_user_a_id_fkey FOREIGN KEY (user_a_id) REFERENCES public.users(id);


--
-- Name: evening_tokens evening_tokens_user_b_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.evening_tokens
    ADD CONSTRAINT evening_tokens_user_b_id_fkey FOREIGN KEY (user_b_id) REFERENCES public.users(id);


--
-- Name: explicit_portals explicit_portals_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.explicit_portals
    ADD CONSTRAINT explicit_portals_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: farm_visit_bookings farm_visit_bookings_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.farm_visit_bookings
    ADD CONSTRAINT farm_visit_bookings_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: farm_visit_bookings farm_visit_bookings_visit_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.farm_visit_bookings
    ADD CONSTRAINT farm_visit_bookings_visit_id_fkey FOREIGN KEY (visit_id) REFERENCES public.farm_visits(id);


--
-- Name: fraise_business_sessions fraise_business_sessions_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_business_sessions
    ADD CONSTRAINT fraise_business_sessions_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.fraise_businesses(id);


--
-- Name: fraise_businesses fraise_businesses_member_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_businesses
    ADD CONSTRAINT fraise_businesses_member_id_fkey FOREIGN KEY (member_id) REFERENCES public.fraise_members(id);


--
-- Name: fraise_claims fraise_claims_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_claims
    ADD CONSTRAINT fraise_claims_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.fraise_events(id);


--
-- Name: fraise_claims fraise_claims_member_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_claims
    ADD CONSTRAINT fraise_claims_member_id_fkey FOREIGN KEY (member_id) REFERENCES public.fraise_members(id);


--
-- Name: fraise_credit_purchases fraise_credit_purchases_member_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_credit_purchases
    ADD CONSTRAINT fraise_credit_purchases_member_id_fkey FOREIGN KEY (member_id) REFERENCES public.fraise_members(id);


--
-- Name: fraise_events fraise_events_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_events
    ADD CONSTRAINT fraise_events_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.fraise_businesses(id);


--
-- Name: fraise_interest fraise_interest_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_interest
    ADD CONSTRAINT fraise_interest_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.fraise_businesses(id);


--
-- Name: fraise_interest fraise_interest_fraise_member_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_interest
    ADD CONSTRAINT fraise_interest_fraise_member_id_fkey FOREIGN KEY (fraise_member_id) REFERENCES public.fraise_members(id);


--
-- Name: fraise_invitations fraise_invitations_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_invitations
    ADD CONSTRAINT fraise_invitations_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.fraise_events(id);


--
-- Name: fraise_invitations fraise_invitations_member_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_invitations
    ADD CONSTRAINT fraise_invitations_member_id_fkey FOREIGN KEY (member_id) REFERENCES public.fraise_members(id);


--
-- Name: fraise_member_resets fraise_member_resets_member_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_member_resets
    ADD CONSTRAINT fraise_member_resets_member_id_fkey FOREIGN KEY (member_id) REFERENCES public.fraise_members(id);


--
-- Name: fraise_member_sessions fraise_member_sessions_member_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_member_sessions
    ADD CONSTRAINT fraise_member_sessions_member_id_fkey FOREIGN KEY (member_id) REFERENCES public.fraise_members(id);


--
-- Name: fraise_messages fraise_messages_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fraise_messages
    ADD CONSTRAINT fraise_messages_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: fund_contributions fund_contributions_from_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fund_contributions
    ADD CONSTRAINT fund_contributions_from_user_id_fkey FOREIGN KEY (from_user_id) REFERENCES public.users(id);


--
-- Name: fund_contributions fund_contributions_to_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.fund_contributions
    ADD CONSTRAINT fund_contributions_to_user_id_fkey FOREIGN KEY (to_user_id) REFERENCES public.users(id);


--
-- Name: gifts gifts_claimed_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gifts
    ADD CONSTRAINT gifts_claimed_by_user_id_fkey FOREIGN KEY (claimed_by_user_id) REFERENCES public.users(id);


--
-- Name: gifts gifts_sender_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gifts
    ADD CONSTRAINT gifts_sender_user_id_fkey FOREIGN KEY (sender_user_id) REFERENCES public.users(id);


--
-- Name: gifts gifts_sticker_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.gifts
    ADD CONSTRAINT gifts_sticker_business_id_fkey FOREIGN KEY (sticker_business_id) REFERENCES public.businesses(id);


--
-- Name: greenhouse_funding greenhouse_funding_greenhouse_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.greenhouse_funding
    ADD CONSTRAINT greenhouse_funding_greenhouse_id_fkey FOREIGN KEY (greenhouse_id) REFERENCES public.greenhouses(id);


--
-- Name: greenhouse_funding greenhouse_funding_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.greenhouse_funding
    ADD CONSTRAINT greenhouse_funding_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: greenhouses greenhouses_founding_patron_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.greenhouses
    ADD CONSTRAINT greenhouses_founding_patron_id_fkey FOREIGN KEY (founding_patron_id) REFERENCES public.users(id);


--
-- Name: harvest_logs harvest_logs_variety_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.harvest_logs
    ADD CONSTRAINT harvest_logs_variety_id_fkey FOREIGN KEY (variety_id) REFERENCES public.varieties(id);


--
-- Name: health_profiles health_profiles_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.health_profiles
    ADD CONSTRAINT health_profiles_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: itineraries itineraries_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itineraries
    ADD CONSTRAINT itineraries_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: itinerary_destinations itinerary_destinations_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itinerary_destinations
    ADD CONSTRAINT itinerary_destinations_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: itinerary_destinations itinerary_destinations_itinerary_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itinerary_destinations
    ADD CONSTRAINT itinerary_destinations_itinerary_id_fkey FOREIGN KEY (itinerary_id) REFERENCES public.itineraries(id) ON DELETE CASCADE;


--
-- Name: itinerary_proposals itinerary_proposals_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itinerary_proposals
    ADD CONSTRAINT itinerary_proposals_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: itinerary_proposals itinerary_proposals_destination_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itinerary_proposals
    ADD CONSTRAINT itinerary_proposals_destination_id_fkey FOREIGN KEY (destination_id) REFERENCES public.itinerary_destinations(id);


--
-- Name: itinerary_proposals itinerary_proposals_itinerary_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itinerary_proposals
    ADD CONSTRAINT itinerary_proposals_itinerary_id_fkey FOREIGN KEY (itinerary_id) REFERENCES public.itineraries(id);


--
-- Name: itinerary_proposals itinerary_proposals_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.itinerary_proposals
    ADD CONSTRAINT itinerary_proposals_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: job_applications job_applications_applicant_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_applications
    ADD CONSTRAINT job_applications_applicant_id_fkey FOREIGN KEY (applicant_id) REFERENCES public.users(id);


--
-- Name: job_applications job_applications_job_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_applications
    ADD CONSTRAINT job_applications_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.job_postings(id);


--
-- Name: job_interviews job_interviews_application_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_interviews
    ADD CONSTRAINT job_interviews_application_id_fkey FOREIGN KEY (application_id) REFERENCES public.job_applications(id);


--
-- Name: job_ledger_entries job_ledger_entries_application_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_ledger_entries
    ADD CONSTRAINT job_ledger_entries_application_id_fkey FOREIGN KEY (application_id) REFERENCES public.job_applications(id);


--
-- Name: job_postings job_postings_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_postings
    ADD CONSTRAINT job_postings_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: key_challenges key_challenges_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.key_challenges
    ADD CONSTRAINT key_challenges_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: kommune_press_applications kommune_press_applications_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.kommune_press_applications
    ADD CONSTRAINT kommune_press_applications_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: kommune_reservations kommune_reservations_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.kommune_reservations
    ADD CONSTRAINT kommune_reservations_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.table_events(id);


--
-- Name: location_funding location_funding_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.location_funding
    ADD CONSTRAINT location_funding_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: location_funding location_funding_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.location_funding
    ADD CONSTRAINT location_funding_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: location_staff location_staff_location_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.location_staff
    ADD CONSTRAINT location_staff_location_id_fkey FOREIGN KEY (location_id) REFERENCES public.locations(id);


--
-- Name: location_staff location_staff_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.location_staff
    ADD CONSTRAINT location_staff_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: locations locations_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.locations
    ADD CONSTRAINT locations_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: market_listings market_listings_vendor_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_listings
    ADD CONSTRAINT market_listings_vendor_id_fkey FOREIGN KEY (vendor_id) REFERENCES public.market_vendors(id);


--
-- Name: market_order_items market_order_items_listing_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_order_items
    ADD CONSTRAINT market_order_items_listing_id_fkey FOREIGN KEY (listing_id) REFERENCES public.market_listings(id);


--
-- Name: market_order_items market_order_items_order_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_order_items
    ADD CONSTRAINT market_order_items_order_id_fkey FOREIGN KEY (order_id) REFERENCES public.market_orders_v2(id);


--
-- Name: market_orders market_orders_market_date_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_orders
    ADD CONSTRAINT market_orders_market_date_id_fkey FOREIGN KEY (market_date_id) REFERENCES public.market_dates(id);


--
-- Name: market_orders market_orders_product_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_orders
    ADD CONSTRAINT market_orders_product_id_fkey FOREIGN KEY (product_id) REFERENCES public.market_products(id);


--
-- Name: market_orders market_orders_stall_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_orders
    ADD CONSTRAINT market_orders_stall_id_fkey FOREIGN KEY (stall_id) REFERENCES public.market_stalls(id);


--
-- Name: market_orders_v2 market_orders_v2_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_orders_v2
    ADD CONSTRAINT market_orders_v2_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: market_products market_products_stall_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_products
    ADD CONSTRAINT market_products_stall_id_fkey FOREIGN KEY (stall_id) REFERENCES public.market_stalls(id);


--
-- Name: market_stalls market_stalls_market_date_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_stalls
    ADD CONSTRAINT market_stalls_market_date_id_fkey FOREIGN KEY (market_date_id) REFERENCES public.market_dates(id);


--
-- Name: market_vendors market_vendors_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.market_vendors
    ADD CONSTRAINT market_vendors_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: meeting_tokens meeting_tokens_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.meeting_tokens
    ADD CONSTRAINT meeting_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: membership_funds membership_funds_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.membership_funds
    ADD CONSTRAINT membership_funds_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: membership_waitlist membership_waitlist_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.membership_waitlist
    ADD CONSTRAINT membership_waitlist_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: memberships memberships_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memberships
    ADD CONSTRAINT memberships_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: memory_requests memory_requests_match_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memory_requests
    ADD CONSTRAINT memory_requests_match_id_fkey FOREIGN KEY (match_id) REFERENCES public.date_matches(id);


--
-- Name: memory_requests memory_requests_user_a_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memory_requests
    ADD CONSTRAINT memory_requests_user_a_id_fkey FOREIGN KEY (user_a_id) REFERENCES public.users(id);


--
-- Name: memory_requests memory_requests_user_b_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memory_requests
    ADD CONSTRAINT memory_requests_user_b_id_fkey FOREIGN KEY (user_b_id) REFERENCES public.users(id);


--
-- Name: messages messages_order_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages
    ADD CONSTRAINT messages_order_id_fkey FOREIGN KEY (order_id) REFERENCES public.orders(id);


--
-- Name: messages messages_recipient_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages
    ADD CONSTRAINT messages_recipient_id_fkey FOREIGN KEY (recipient_id) REFERENCES public.users(id);


--
-- Name: messages messages_sender_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages
    ADD CONSTRAINT messages_sender_id_fkey FOREIGN KEY (sender_id) REFERENCES public.users(id);


--
-- Name: nfc_connections nfc_connections_user_a_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.nfc_connections
    ADD CONSTRAINT nfc_connections_user_a_fkey FOREIGN KEY (user_a) REFERENCES public.users(id);


--
-- Name: nfc_connections nfc_connections_user_b_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.nfc_connections
    ADD CONSTRAINT nfc_connections_user_b_fkey FOREIGN KEY (user_b) REFERENCES public.users(id);


--
-- Name: nfc_pairing_tokens nfc_pairing_tokens_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.nfc_pairing_tokens
    ADD CONSTRAINT nfc_pairing_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: node_applications node_applications_applicant_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.node_applications
    ADD CONSTRAINT node_applications_applicant_user_id_fkey FOREIGN KEY (applicant_user_id) REFERENCES public.users(id);


--
-- Name: node_applications node_applications_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.node_applications
    ADD CONSTRAINT node_applications_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: notifications notifications_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: one_time_pre_keys one_time_pre_keys_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.one_time_pre_keys
    ADD CONSTRAINT one_time_pre_keys_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: order_splits order_splits_order_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.order_splits
    ADD CONSTRAINT order_splits_order_id_fkey FOREIGN KEY (order_id) REFERENCES public.orders(id);


--
-- Name: order_splits order_splits_split_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.order_splits
    ADD CONSTRAINT order_splits_split_user_id_fkey FOREIGN KEY (split_user_id) REFERENCES public.users(id);


--
-- Name: orders orders_batch_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.orders
    ADD CONSTRAINT orders_batch_id_fkey FOREIGN KEY (batch_id) REFERENCES public.batches(id);


--
-- Name: orders orders_location_id_locations_id_fk; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.orders
    ADD CONSTRAINT orders_location_id_locations_id_fk FOREIGN KEY (location_id) REFERENCES public.locations(id);


--
-- Name: orders orders_menu_item_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.orders
    ADD CONSTRAINT orders_menu_item_id_fkey FOREIGN KEY (menu_item_id) REFERENCES public.business_menu_items(id);


--
-- Name: orders orders_time_slot_id_time_slots_id_fk; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.orders
    ADD CONSTRAINT orders_time_slot_id_time_slots_id_fk FOREIGN KEY (time_slot_id) REFERENCES public.time_slots(id);


--
-- Name: orders orders_variety_id_varieties_id_fk; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.orders
    ADD CONSTRAINT orders_variety_id_varieties_id_fk FOREIGN KEY (variety_id) REFERENCES public.varieties(id);


--
-- Name: orders orders_worker_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.orders
    ADD CONSTRAINT orders_worker_id_fkey FOREIGN KEY (worker_id) REFERENCES public.users(id);


--
-- Name: pending_connections pending_connections_user_a_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pending_connections
    ADD CONSTRAINT pending_connections_user_a_id_fkey FOREIGN KEY (user_a_id) REFERENCES public.users(id);


--
-- Name: pending_connections pending_connections_user_b_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pending_connections
    ADD CONSTRAINT pending_connections_user_b_id_fkey FOREIGN KEY (user_b_id) REFERENCES public.users(id);


--
-- Name: personal_toilets personal_toilets_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.personal_toilets
    ADD CONSTRAINT personal_toilets_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: personalized_menus personalized_menus_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.personalized_menus
    ADD CONSTRAINT personalized_menus_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: personalized_menus personalized_menus_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.personalized_menus
    ADD CONSTRAINT personalized_menus_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: platform_messages platform_messages_recipient_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.platform_messages
    ADD CONSTRAINT platform_messages_recipient_id_fkey FOREIGN KEY (recipient_id) REFERENCES public.users(id);


--
-- Name: platform_messages platform_messages_reply_to_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.platform_messages
    ADD CONSTRAINT platform_messages_reply_to_id_fkey FOREIGN KEY (reply_to_id) REFERENCES public.platform_messages(id);


--
-- Name: platform_messages platform_messages_sender_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.platform_messages
    ADD CONSTRAINT platform_messages_sender_id_fkey FOREIGN KEY (sender_id) REFERENCES public.users(id);


--
-- Name: popup_food_orders popup_food_orders_buyer_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_food_orders
    ADD CONSTRAINT popup_food_orders_buyer_user_id_fkey FOREIGN KEY (buyer_user_id) REFERENCES public.users(id);


--
-- Name: popup_food_orders popup_food_orders_menu_item_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_food_orders
    ADD CONSTRAINT popup_food_orders_menu_item_id_fkey FOREIGN KEY (menu_item_id) REFERENCES public.business_menu_items(id);


--
-- Name: popup_food_orders popup_food_orders_popup_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_food_orders
    ADD CONSTRAINT popup_food_orders_popup_id_fkey FOREIGN KEY (popup_id) REFERENCES public.businesses(id);


--
-- Name: popup_food_orders popup_food_orders_recipient_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_food_orders
    ADD CONSTRAINT popup_food_orders_recipient_user_id_fkey FOREIGN KEY (recipient_user_id) REFERENCES public.users(id);


--
-- Name: popup_merch_items popup_merch_items_popup_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_merch_items
    ADD CONSTRAINT popup_merch_items_popup_id_fkey FOREIGN KEY (popup_id) REFERENCES public.businesses(id);


--
-- Name: popup_merch_orders popup_merch_orders_buyer_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_merch_orders
    ADD CONSTRAINT popup_merch_orders_buyer_user_id_fkey FOREIGN KEY (buyer_user_id) REFERENCES public.users(id);


--
-- Name: popup_merch_orders popup_merch_orders_item_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_merch_orders
    ADD CONSTRAINT popup_merch_orders_item_id_fkey FOREIGN KEY (item_id) REFERENCES public.popup_merch_items(id);


--
-- Name: popup_merch_orders popup_merch_orders_popup_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_merch_orders
    ADD CONSTRAINT popup_merch_orders_popup_id_fkey FOREIGN KEY (popup_id) REFERENCES public.businesses(id);


--
-- Name: popup_merch_orders popup_merch_orders_recipient_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.popup_merch_orders
    ADD CONSTRAINT popup_merch_orders_recipient_user_id_fkey FOREIGN KEY (recipient_user_id) REFERENCES public.users(id);


--
-- Name: portal_access portal_access_buyer_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portal_access
    ADD CONSTRAINT portal_access_buyer_id_fkey FOREIGN KEY (buyer_id) REFERENCES public.users(id);


--
-- Name: portal_access portal_access_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portal_access
    ADD CONSTRAINT portal_access_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id);


--
-- Name: portal_consents portal_consents_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portal_consents
    ADD CONSTRAINT portal_consents_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: portal_content portal_content_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portal_content
    ADD CONSTRAINT portal_content_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: portrait_license_requests portrait_license_requests_token_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_license_requests
    ADD CONSTRAINT portrait_license_requests_token_id_fkey FOREIGN KEY (token_id) REFERENCES public.portrait_tokens(id);


--
-- Name: portrait_licenses portrait_licenses_request_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_licenses
    ADD CONSTRAINT portrait_licenses_request_id_fkey FOREIGN KEY (request_id) REFERENCES public.portrait_license_requests(id);


--
-- Name: portrait_licenses portrait_licenses_token_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_licenses
    ADD CONSTRAINT portrait_licenses_token_id_fkey FOREIGN KEY (token_id) REFERENCES public.portrait_tokens(id);


--
-- Name: portrait_token_listings portrait_token_listings_buyer_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_token_listings
    ADD CONSTRAINT portrait_token_listings_buyer_user_id_fkey FOREIGN KEY (buyer_user_id) REFERENCES public.users(id);


--
-- Name: portrait_token_listings portrait_token_listings_seller_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_token_listings
    ADD CONSTRAINT portrait_token_listings_seller_user_id_fkey FOREIGN KEY (seller_user_id) REFERENCES public.users(id);


--
-- Name: portrait_token_listings portrait_token_listings_token_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_token_listings
    ADD CONSTRAINT portrait_token_listings_token_id_fkey FOREIGN KEY (token_id) REFERENCES public.portrait_tokens(id);


--
-- Name: portrait_tokens portrait_tokens_minted_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_tokens
    ADD CONSTRAINT portrait_tokens_minted_by_fkey FOREIGN KEY (minted_by) REFERENCES public.users(id);


--
-- Name: portrait_tokens portrait_tokens_original_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_tokens
    ADD CONSTRAINT portrait_tokens_original_owner_id_fkey FOREIGN KEY (original_owner_id) REFERENCES public.users(id);


--
-- Name: portrait_tokens portrait_tokens_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.portrait_tokens
    ADD CONSTRAINT portrait_tokens_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id);


--
-- Name: preorders preorders_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.preorders
    ADD CONSTRAINT preorders_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: preorders preorders_variety_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.preorders
    ADD CONSTRAINT preorders_variety_id_fkey FOREIGN KEY (variety_id) REFERENCES public.varieties(id);


--
-- Name: promotion_deliveries promotion_deliveries_promotion_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.promotion_deliveries
    ADD CONSTRAINT promotion_deliveries_promotion_id_fkey FOREIGN KEY (promotion_id) REFERENCES public.business_promotions(id);


--
-- Name: promotion_deliveries promotion_deliveries_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.promotion_deliveries
    ADD CONSTRAINT promotion_deliveries_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: provenance_tokens provenance_tokens_greenhouse_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provenance_tokens
    ADD CONSTRAINT provenance_tokens_greenhouse_id_fkey FOREIGN KEY (greenhouse_id) REFERENCES public.greenhouses(id);


--
-- Name: provenance_tokens provenance_tokens_location_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provenance_tokens
    ADD CONSTRAINT provenance_tokens_location_id_fkey FOREIGN KEY (location_id) REFERENCES public.businesses(id);


--
-- Name: referral_codes referral_codes_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.referral_codes
    ADD CONSTRAINT referral_codes_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: referrals referrals_referee_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.referrals
    ADD CONSTRAINT referrals_referee_user_id_fkey FOREIGN KEY (referee_user_id) REFERENCES public.users(id);


--
-- Name: referrals referrals_referrer_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.referrals
    ADD CONSTRAINT referrals_referrer_user_id_fkey FOREIGN KEY (referrer_user_id) REFERENCES public.users(id);


--
-- Name: reservation_bookings reservation_bookings_guest_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reservation_bookings
    ADD CONSTRAINT reservation_bookings_guest_user_id_fkey FOREIGN KEY (guest_user_id) REFERENCES public.users(id);


--
-- Name: reservation_bookings reservation_bookings_initiator_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reservation_bookings
    ADD CONSTRAINT reservation_bookings_initiator_user_id_fkey FOREIGN KEY (initiator_user_id) REFERENCES public.users(id);


--
-- Name: reservation_bookings reservation_bookings_offer_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reservation_bookings
    ADD CONSTRAINT reservation_bookings_offer_id_fkey FOREIGN KEY (offer_id) REFERENCES public.reservation_offers(id);


--
-- Name: reservation_offers reservation_offers_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.reservation_offers
    ADD CONSTRAINT reservation_offers_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: season_patronages season_patronages_location_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.season_patronages
    ADD CONSTRAINT season_patronages_location_id_fkey FOREIGN KEY (location_id) REFERENCES public.businesses(id);


--
-- Name: season_patronages season_patronages_patron_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.season_patronages
    ADD CONSTRAINT season_patronages_patron_user_id_fkey FOREIGN KEY (patron_user_id) REFERENCES public.users(id);


--
-- Name: season_patronages season_patronages_requested_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.season_patronages
    ADD CONSTRAINT season_patronages_requested_by_fkey FOREIGN KEY (requested_by) REFERENCES public.users(id);


--
-- Name: standing_order_transfers standing_order_transfers_from_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_order_transfers
    ADD CONSTRAINT standing_order_transfers_from_user_id_fkey FOREIGN KEY (from_user_id) REFERENCES public.users(id);


--
-- Name: standing_order_transfers standing_order_transfers_standing_order_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_order_transfers
    ADD CONSTRAINT standing_order_transfers_standing_order_id_fkey FOREIGN KEY (standing_order_id) REFERENCES public.standing_orders(id);


--
-- Name: standing_order_transfers standing_order_transfers_to_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_order_transfers
    ADD CONSTRAINT standing_order_transfers_to_user_id_fkey FOREIGN KEY (to_user_id) REFERENCES public.users(id);


--
-- Name: standing_order_waitlist standing_order_waitlist_referred_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_order_waitlist
    ADD CONSTRAINT standing_order_waitlist_referred_by_user_id_fkey FOREIGN KEY (referred_by_user_id) REFERENCES public.users(id);


--
-- Name: standing_order_waitlist standing_order_waitlist_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_order_waitlist
    ADD CONSTRAINT standing_order_waitlist_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: standing_orders standing_orders_gifted_by_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.standing_orders
    ADD CONSTRAINT standing_orders_gifted_by_user_id_fkey FOREIGN KEY (gifted_by_user_id) REFERENCES public.users(id);


--
-- Name: table_booking_tokens table_booking_tokens_booking_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_booking_tokens
    ADD CONSTRAINT table_booking_tokens_booking_id_fkey FOREIGN KEY (booking_id) REFERENCES public.table_bookings(id);


--
-- Name: table_bookings table_bookings_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_bookings
    ADD CONSTRAINT table_bookings_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.table_events(id);


--
-- Name: table_events table_events_instructor_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_events
    ADD CONSTRAINT table_events_instructor_id_fkey FOREIGN KEY (instructor_id) REFERENCES public.table_instructors(id);


--
-- Name: table_events table_events_parent_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.table_events
    ADD CONSTRAINT table_events_parent_event_id_fkey FOREIGN KEY (parent_event_id) REFERENCES public.table_events(id);


--
-- Name: tasting_feed_reactions tasting_feed_reactions_entry_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasting_feed_reactions
    ADD CONSTRAINT tasting_feed_reactions_entry_id_fkey FOREIGN KEY (entry_id) REFERENCES public.tasting_journal(id) ON DELETE CASCADE;


--
-- Name: tasting_feed_reactions tasting_feed_reactions_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasting_feed_reactions
    ADD CONSTRAINT tasting_feed_reactions_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: tasting_journal tasting_journal_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasting_journal
    ADD CONSTRAINT tasting_journal_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: tasting_journal tasting_journal_variety_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tasting_journal
    ADD CONSTRAINT tasting_journal_variety_id_fkey FOREIGN KEY (variety_id) REFERENCES public.varieties(id);


--
-- Name: time_slots time_slots_location_id_locations_id_fk; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.time_slots
    ADD CONSTRAINT time_slots_location_id_locations_id_fk FOREIGN KEY (location_id) REFERENCES public.locations(id);


--
-- Name: toilet_visits toilet_visits_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.toilet_visits
    ADD CONSTRAINT toilet_visits_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: toilet_visits toilet_visits_personal_toilet_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.toilet_visits
    ADD CONSTRAINT toilet_visits_personal_toilet_id_fkey FOREIGN KEY (personal_toilet_id) REFERENCES public.personal_toilets(id);


--
-- Name: toilet_visits toilet_visits_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.toilet_visits
    ADD CONSTRAINT toilet_visits_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: typing_indicators typing_indicators_contact_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.typing_indicators
    ADD CONSTRAINT typing_indicators_contact_id_fkey FOREIGN KEY (contact_id) REFERENCES public.users(id);


--
-- Name: typing_indicators typing_indicators_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.typing_indicators
    ADD CONSTRAINT typing_indicators_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: user_business_visits user_business_visits_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_business_visits
    ADD CONSTRAINT user_business_visits_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id) ON DELETE CASCADE;


--
-- Name: user_business_visits user_business_visits_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_business_visits
    ADD CONSTRAINT user_business_visits_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: user_earnings user_earnings_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_earnings
    ADD CONSTRAINT user_earnings_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: user_follows user_follows_followee_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_follows
    ADD CONSTRAINT user_follows_followee_id_fkey FOREIGN KEY (followee_id) REFERENCES public.users(id);


--
-- Name: user_follows user_follows_follower_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_follows
    ADD CONSTRAINT user_follows_follower_id_fkey FOREIGN KEY (follower_id) REFERENCES public.users(id);


--
-- Name: user_keys user_keys_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_keys
    ADD CONSTRAINT user_keys_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: user_map_entries user_map_entries_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_map_entries
    ADD CONSTRAINT user_map_entries_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id) ON DELETE CASCADE;


--
-- Name: user_map_entries user_map_entries_map_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_map_entries
    ADD CONSTRAINT user_map_entries_map_id_fkey FOREIGN KEY (map_id) REFERENCES public.user_maps(id) ON DELETE CASCADE;


--
-- Name: user_maps user_maps_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_maps
    ADD CONSTRAINT user_maps_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: user_saves user_saves_saved_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_saves
    ADD CONSTRAINT user_saves_saved_user_id_fkey FOREIGN KEY (saved_user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: user_saves user_saves_saver_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_saves
    ADD CONSTRAINT user_saves_saver_id_fkey FOREIGN KEY (saver_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: users users_business_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_business_id_fkey FOREIGN KEY (business_id) REFERENCES public.businesses(id);


--
-- Name: varieties varieties_location_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.varieties
    ADD CONSTRAINT varieties_location_id_fkey FOREIGN KEY (location_id) REFERENCES public.locations(id);


--
-- Name: variety_drops variety_drops_variety_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_drops
    ADD CONSTRAINT variety_drops_variety_id_fkey FOREIGN KEY (variety_id) REFERENCES public.varieties(id);


--
-- Name: variety_profiles variety_profiles_variety_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_profiles
    ADD CONSTRAINT variety_profiles_variety_id_fkey FOREIGN KEY (variety_id) REFERENCES public.varieties(id);


--
-- Name: variety_reviews variety_reviews_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_reviews
    ADD CONSTRAINT variety_reviews_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: variety_reviews variety_reviews_variety_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_reviews
    ADD CONSTRAINT variety_reviews_variety_id_fkey FOREIGN KEY (variety_id) REFERENCES public.varieties(id);


--
-- Name: variety_seasons variety_seasons_variety_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.variety_seasons
    ADD CONSTRAINT variety_seasons_variety_id_fkey FOREIGN KEY (variety_id) REFERENCES public.varieties(id);


--
-- Name: walk_in_tokens walk_in_tokens_location_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.walk_in_tokens
    ADD CONSTRAINT walk_in_tokens_location_id_fkey FOREIGN KEY (location_id) REFERENCES public.locations(id);


--
-- Name: walk_in_tokens walk_in_tokens_variety_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.walk_in_tokens
    ADD CONSTRAINT walk_in_tokens_variety_id_fkey FOREIGN KEY (variety_id) REFERENCES public.varieties(id);


--
-- Name: webhook_subscriptions webhook_subscriptions_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhook_subscriptions
    ADD CONSTRAINT webhook_subscriptions_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- PostgreSQL database dump complete
--


